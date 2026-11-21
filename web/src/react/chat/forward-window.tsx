import { useEffect, useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { navigate } from '../../navigation';
import { workspaceSession, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { existingDirect } from './direct-conversation';
import { forwardedOnly } from './forward-message';
import { ForwardRecipient, type ForwardTarget } from './forward-recipient';
import { chatRequest, filesOnlyContent, type Attachment, type Conversation, type Directory, type Message } from './model';
import { fileSize } from '../results/file-kind';
import { MAX_BYTES, MAX_FILES, uploadChatFile } from './upload-chat-file';
import './forward-window.css';

/** A document you add, kept here until Forward is pressed. */
type Draft = { key: string; file: File };


function FileIcon() {
  return <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" /><path d="M14 3v5h5" />
  </svg>;
}

/**
 * Forward one message to a person or an agent, in your direct chat with them:
 * who it goes to, the message with the files that came with it, then your own
 * words and documents. The message's files are copied by the server; your
 * documents are uploaded when you press Forward, so cancelling leaves nothing.
 */
export function ForwardMessage({ message, name, place, directory, close }: {
  message: Message; name: string; place: string; directory: Directory; close: () => void;
}) {
  const [target, setTarget] = useState<ForwardTarget>();
  const [note, setNote] = useState('');
  const [left, setLeft] = useState<string[]>([]);
  const [mine, setMine] = useState<Draft[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [problem, setProblem] = useState('');
  const [uploading, setUploading] = useState(false);
  const [chats, setChats] = useState<Conversation[]>([]);
  const mutation = useMutation();
  // One ID per exact forward, so a retried send after a lost answer posts once.
  const attempt = useRef<{ key: string; id: string; chat: string; room?: string }>(undefined);
  // A document already stored in a chat is not stored again by a retry.
  const stored = useRef(new Map<string, Attachment>());
  const sending = useRef<AbortController>(undefined);
  const search = useRef<HTMLInputElement>(null), words = useRef<HTMLTextAreaElement>(null), picker = useRef<HTMLInputElement>(null);
  const me = workspaceSession?.user?.id;
  // The person's conversations order the list; without them it is the directory's order.
  useEffect(() => {
    const reading = new AbortController();
    chatRequest<{ chats?: Conversation[] }>('/chats', undefined, reading.signal)
      .then(data => { if (!reading.signal.aborted) setChats(data.chats ?? []); }, () => { /* The list keeps the directory's order. */ });
    return () => { reading.abort(); sending.current?.abort(); };
  }, []);

  const everyone: ForwardTarget[] = [
    ...directory.agents.filter(agent => agent.audience !== 'only_me' || agent.yours).map(agent => ({ ...agent, kind: 'agent' as const })),
    ...directory.users.filter(user => user.id !== me).map(user => ({ ...user, kind: 'user' as const }))];
  // The conversation the message is in is not where it is likely to go.
  const lastTalked = (item: ForwardTarget) => { const direct = existingDirect(chats, item, me); return direct && direct.id !== message.chat ? direct.activity ?? 0 : 0; };
  const ranked = everyone.map(item => ({ item, at: lastTalked(item) })).sort((a, b) => b.at - a.at);

  // A forward without words of its own passes on what it forwarded.
  const passedOn = message.forwarded && message.content === forwardedOnly ? message.forwarded : undefined;
  const files = message.attachments ?? [];
  const text = passedOn ? passedOn.text : message.content === filesOnlyContent && files.length ? '' : message.content;
  const from = passedOn ?? { name, kind: message.kind === 'agent' ? 'agent' as const : 'user' as const, place, at: message.at };
  const when = new Date(from.at * 1000).toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  const long = text.length > 240 || text.split('\n').length > 4;
  const included = files.filter(file => !left.includes(file.id));

  const agent = target?.kind === 'agent';
  const needsNote = agent && !note.trim();
  const nothing = !text && !included.length;
  const hint = !target ? 'Choose who it goes to' : nothing ? 'Include a file to forward' : needsNote ? 'Say what you need first' : '';
  const label = uploading ? 'Uploading…' : mutation.busy ? 'Forwarding…' : target ? `Forward to ${target.name}` : 'Forward';

  const add = (chosen: File[]) => {
    let next = mine, refused = '';
    for (const file of chosen) {
      if (!file.size || file.size > MAX_BYTES) { refused = `${file.name} is empty or over 10 MiB.`; continue; }
      if (included.length + next.length >= MAX_FILES) { refused = 'A message can carry at most eight files.'; break; }
      next = [...next, { key: crypto.randomUUID(), file }];
    }
    setMine(next); setProblem(refused);
  };
  const include = (id: string) => {
    if (included.length + mine.length >= MAX_FILES) { setProblem('A message can carry at most eight files.'); return; }
    setLeft(left.filter(item => item !== id)); setProblem('');
  };
  const send = () => void mutation.run(async () => {
    if (!target || needsNote || nothing) return;
    const carried = included.map(file => file.id);
    const key = JSON.stringify([target.kind, target.id, note.trim(), carried, mine.map(draft => draft.key)]);
    if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID(), chat: crypto.randomUUID() };
    const { id, chat } = attempt.current;
    const attachments: string[] = [];
    if (mine.length) {
      setUploading(true); sending.current = new AbortController();
      try {
        // A file is stored in the conversation it is shared in, so that chat comes first.
        const room = attempt.current.room ??= existingDirect(chats, target, me)?.id ?? (await chatRequest<{ id: string }>('/chats', {
          id: chat, kind: 'direct', name: target.name, description: '',
          users: target.kind === 'user' ? [target.id] : [], agents: target.kind === 'agent' ? [target.id] : [],
        })).id;
        for (const draft of mine) {
          const slot = `${room}:${draft.key}`;
          const saved = stored.current.get(slot) ?? await uploadChatFile(room, draft.file, sending.current.signal);
          stored.current.set(slot, saved); attachments.push(saved.id);
        }
      } finally { setUploading(false); }
    }
    const result = await chatRequest<{ chat: string }>(`/chats/${encodeURIComponent(message.chat ?? '')}/messages/${encodeURIComponent(message.id)}/forward`,
      { id, chat, to: { kind: target.kind, id: target.id }, note: note.trim(), files: carried, attachments });
    close();
    navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(result.chat)}`));
  });

  return <Dialog title="Forward message" busy={mutation.busy} status={mutation.status} close={close} initialFocus={search}>
    <div className="chat-forward">
      <ForwardRecipient candidates={ranked.map(entry => entry.item)} recent={ranked.some(entry => entry.at > 0)} target={target} input={search}
        choose={next => { setTarget(next); setProblem(''); if (next) words.current?.focus(); }} />
      <section className="chat-forwarded" aria-label={`Message from ${from.name}`}>
        <header><span><strong>{from.name}</strong></span>{from.kind === 'agent' && <span className="ai-tag">AI</span>}<span>· {[from.place, when].filter(Boolean).join(' · ')}</span></header>
        {text && <div className={`message-markdown${long && !expanded ? ' chat-forward-clamped' : ''}`} dangerouslySetInnerHTML={{ __html: messageHtml(text) }} />}
        {long && <button type="button" className="chat-forward-expand" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? 'Show less' : 'Show all'}</button>}
        {files.length > 0 && <ul className="chat-forward-files" aria-label="Files with this message">{files.map(file => {
          const out = left.includes(file.id);
          return <li key={file.id} className={out ? 'left-out' : undefined}><FileIcon /><span className="chat-forward-file-name">{file.name}</span><small>{fileSize(file.size)}</small>
            {out ? <button type="button" className="chat-forward-include" aria-label={`Include ${file.name}`} onClick={() => include(file.id)}>Include</button>
              : <button type="button" aria-label={`Leave out ${file.name}`} title="Leave out" onClick={() => { setLeft([...left, file.id]); setProblem(''); }}>×</button>}
          </li>;
        })}</ul>}
      </section>
      <div className="chat-forward-box">
        <textarea ref={words} rows={2} maxLength={4000} value={note} onChange={event => setNote(event.target.value)}
          aria-label={agent ? `What do you need from ${target.name}?` : 'Your message (optional)'}
          placeholder={agent ? `What do you need from ${target.name}?` : 'Add a message (optional)'}
          onKeyDown={event => { if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); send(); } }} />
        {mine.length > 0 && <ul className="chat-forward-files" aria-label="Your documents">{mine.map(draft =>
          <li key={draft.key}><FileIcon /><span className="chat-forward-file-name">{draft.file.name}</span><small>{fileSize(draft.file.size)}</small>
            <button type="button" aria-label={`Remove ${draft.file.name}`} title="Remove" onClick={() => { setMine(mine.filter(item => item.key !== draft.key)); setProblem(''); }}>×</button></li>)}</ul>}
        <div className="chat-forward-actions">
          <input ref={picker} type="file" multiple hidden aria-label="Choose documents" onChange={event => { add([...event.target.files ?? []]); event.target.value = ''; }} />
          <button type="button" className="chat-forward-attach" aria-label="Attach a document" title="Attach a document" onClick={() => picker.current?.click()}>
            <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="m8 12 7-7a4 4 0 0 1 6 6l-10 10a6 6 0 0 1-8-8L13 3" /></svg>
          </button>
          <span className="chat-forward-hint">{hint}</span>
          <Button primary disabled={!target || needsNote || nothing || mutation.busy} onClick={send}>{label}</Button>
        </div>
      </div>
      {problem && <p className="chat-error" role="alert">{problem}</p>}
      {agent && <p className="chat-forward-note">{target.yours && target.audience === 'only_me'
        ? `Only you read your chat with ${target.name}.`
        : `${target.name}’s owners and admins can see what you forward.`}</p>}
    </div>
  </Dialog>;
}
