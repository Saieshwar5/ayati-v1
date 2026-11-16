import { useRef, useState } from 'react';
import { messageHtml } from '../../markdown';
import { navigate } from '../../navigation';
import { workspaceSession, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { ParticipantFace, ParticipantName } from './conversation-face';
import { chatRequest, type Directory, type Message, type Participant } from './model';
import './forward-message.css';

/** A copy of the message a forward carries, as it was when forwarded (schema 125). */
export type Forwarded = { name: string; kind: 'user' | 'agent'; place: string; at: number; text: string };

/** Stored as the content of a forward sent to a person without words of its own. */
export const forwardedOnly = 'Forwarded message';

export function ForwardIcon() {
  return <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="m14 5 6 6-6 6" /><path d="M20 11H9a5 5 0 0 0-5 5v3" />
  </svg>;
}

/** The forwarded message inside the message that carries it: who wrote it, where and when. */
export function ForwardedBlock({ forwarded }: { forwarded: Forwarded }) {
  const when = new Date(forwarded.at * 1000).toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  return <section className="chat-forwarded" aria-label={`Forwarded message from ${forwarded.name}`}>
    <header><ForwardIcon /><span>Forwarded from <strong>{forwarded.name}</strong></span>{forwarded.kind === 'agent' && <span className="ai-tag">AI</span>}<span>· {[forwarded.place, when].filter(Boolean).join(' · ')}</span></header>
    <div className="message-markdown" dangerouslySetInnerHTML={{ __html: messageHtml(forwarded.text) }} />
  </section>;
}

type Target = Participant & { kind: 'user' | 'agent' };

/** Forward one message to a person or an agent, in your direct chat with them. */
export function ForwardMessage({ message, name, place, directory, close }: {
  message: Message; name: string; place: string; directory: Directory; close: () => void;
}) {
  const [query, setQuery] = useState('');
  const [target, setTarget] = useState<Target>();
  const [note, setNote] = useState('');
  const mutation = useMutation();
  // One ID per exact forward, so a retried send after a lost answer posts once.
  const attempt = useRef<{ key: string; id: string; chat: string }>(undefined);
  const me = workspaceSession?.user?.id;
  const match = (item: Participant) => item.name.toLowerCase().includes(query.trim().toLowerCase());
  const agents = directory.agents.filter(agent => agent.audience !== 'only_me' || agent.yours).filter(match).map(agent => ({ ...agent, kind: 'agent' as const }));
  const people = directory.users.filter(user => user.id !== me).filter(match).map(user => ({ ...user, kind: 'user' as const }));
  const needsNote = target?.kind === 'agent' && !note.trim();
  const row = (item: Target) => <button type="button" className="chat-contact" key={`${item.kind}:${item.id}`}
    aria-pressed={target?.id === item.id && target.kind === item.kind} onClick={() => setTarget(item)}>
    <ParticipantFace kind={item.kind} who={item} /><span><ParticipantName kind={item.kind} name={item.name} /><small>{item.kind === 'agent' ? item.role : item.title}</small></span>
  </button>;
  const send = () => void mutation.run(async () => {
    if (!target || needsNote) return;
    const key = JSON.stringify([target.kind, target.id, note.trim()]);
    if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID(), chat: crypto.randomUUID() };
    const result = await chatRequest<{ chat: string }>(`/chats/${encodeURIComponent(message.chat ?? '')}/messages/${encodeURIComponent(message.id)}/forward`,
      { id: attempt.current.id, chat: attempt.current.chat, to: { kind: target.kind, id: target.id }, note: note.trim() });
    close();
    navigate(workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(result.chat)}`));
  });
  const preview: Forwarded = { name, kind: message.kind === 'agent' ? 'agent' : 'user', place, at: message.at, text: message.content };
  const label = mutation.busy ? 'Forwarding…' : target ? `Forward to ${target.name}` : 'Forward';
  return <Dialog title="Forward message" busy={mutation.busy} status={mutation.status} close={close}
    footer={<><Button onClick={close} disabled={mutation.busy}>Cancel</Button><Button primary disabled={!target || needsNote || mutation.busy} onClick={send}>{label}</Button></>}>
    <div className="chat-forward">
      <label>Forward to<input type="search" autoFocus value={query} onChange={event => setQuery(event.target.value)} placeholder="Search people and agents…" /></label>
      <div className="chat-forward-list" role="group" aria-label="People and agents">
        {agents.length > 0 && <h3>Agents</h3>}{agents.map(row)}
        {people.length > 0 && <h3>People</h3>}{people.map(row)}
        {!agents.length && !people.length && <p className="chat-help">No one matches “{query.trim()}”.</p>}
      </div>
      <label>{target?.kind === 'agent' ? `What do you need from ${target.name}?` : 'Add a message (optional)'}
        <textarea value={note} maxLength={4000} onChange={event => setNote(event.target.value)}
          placeholder={target?.kind === 'agent' ? 'For example: find out why this is unpaid' : 'Add a message'} /></label>
      <ForwardedBlock forwarded={preview} />
      {target?.kind === 'agent' && (target.yours && target.audience === 'only_me'
        ? <p className="chat-forward-note quiet">Only you read your chat with {target.name}.</p>
        : <p className="chat-forward-note">{target.name}’s owners and admins can see what you forward. {target.name} treats the forwarded text as information and your words as the request.</p>)}
    </div>
  </Dialog>;
}
