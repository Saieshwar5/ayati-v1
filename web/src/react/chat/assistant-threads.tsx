import { useRef, useState } from 'react';
import { navigate } from '../../navigation';
import { workspaceSession, workspaceUrl } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { chatRequest, type Directory } from './model';
import { setThreadState, useThreads, type AssistantThread } from './assistant-threads-model';
import './assistant-threads.css';

const chatUrl = (chat: string) => workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(chat)}`);
const withName = (thread: AssistantThread) => thread.name ?? (thread.kind === 'agent' ? 'an agent' : 'a member');

export function ThreadIcon() {
  return <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M7 8h10M7 12h6" /><path d="M4 5h16v11H9l-5 4z" />
  </svg>;
}

/** Stop, resume, start or delete one thread; one request at a time. */
function useThreadActions() {
  const { reload } = useThreads();
  const mutation = useMutation();
  const set = (thread: AssistantThread, state: 'running' | 'stopped' | 'deleted', then?: () => void) => void mutation.run(async () => {
    await setThreadState(thread.id, state);
    reload(); then?.();
  });
  return { ...mutation, set };
}

/** Under your assistant in the chat list: its running threads and the way to all of them. */
export function ThreadRows({ selected, manage, settings }: { selected: string | null; manage: () => void; settings?: () => void }) {
  const { threads } = useThreads();
  const running = threads.filter(thread => thread.state === 'running' && thread.chat);
  const suggested = threads.filter(thread => thread.state === 'suggested').length;
  return <div className="chat-thread-rows">
    {running.map(thread => <a key={thread.id} className="chat-thread-row" href={chatUrl(thread.chat!)} aria-current={selected === thread.chat ? 'page' : undefined}
      title={`${thread.title} · with ${withName(thread)}`}>
      <ThreadIcon /><span>{thread.title}<small>with {withName(thread)}{thread.working ? ' · writing…' : ''}</small></span>
    </a>)}
    <Button className="chat-thread-manage" onClick={manage}>
      <ThreadIcon />Threads{suggested > 0 && <small className="chat-unread" aria-label={`${suggested} suggested`}>{suggested}</small>}
    </Button>
    {settings && <Button className="chat-thread-manage" onClick={settings}>
      <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" aria-hidden="true"><circle cx="12" cy="8" r="3.5" /><path d="M5 20a7 7 0 0 1 14 0" /></svg>About you &amp; checks
    </Button>}
  </div>;
}

/** At the top of a chat: the thread running or stopped there, and in your
 * assistant's own chat, the threads it suggests. */
export function ThreadBanner({ chat, assistantName }: { chat: string; assistantName: string }) {
  const { threads, room } = useThreads();
  const actions = useThreadActions();
  const here = threads.filter(thread => thread.chat === chat && thread.state !== 'suggested');
  const current = here.find(thread => thread.state === 'running') ?? here[0];
  const suggestions = chat === room ? threads.filter(thread => thread.state === 'suggested') : [];
  if (!current && !suggestions.length) return null;
  return <div className="chat-thread-banners">
    {current && <div className={`chat-thread-banner ${current.state}`} role="status">
      <ThreadIcon />
      <span>{current.state === 'running'
        ? <><strong>{assistantName} is answering {withName(current)} for you</strong> · {current.title}{current.working ? ' · writing a reply…' : ''}<small>Its messages appear here as yours. You can write here too.</small></>
        : <><strong>Thread stopped</strong> · {current.title}<small>{current.detail || `${assistantName} sends nothing until you resume it.`}</small></>}</span>
      {current.state === 'running'
        ? <Button disabled={actions.busy} onClick={() => actions.set(current, 'stopped')}>Stop</Button>
        : <><Button disabled={actions.busy} onClick={() => actions.set(current, 'running')}>Resume</Button><Button disabled={actions.busy} onClick={() => actions.set(current, 'deleted')}>Delete</Button></>}
    </div>}
    {suggestions.map(thread => <div key={thread.id} className="chat-thread-banner suggested">
      <ThreadIcon />
      <span><strong>{assistantName} suggests a thread with {withName(thread)}</strong> · {thread.title}
        <q>{thread.opening}</q>
        <small>If you start it, this is sent as your message, and {assistantName} keeps answering for you until you stop it.</small></span>
      <Button primary disabled={actions.busy} onClick={() => actions.set(thread, 'running')}>Start thread</Button>
      <Button disabled={actions.busy} onClick={() => actions.set(thread, 'deleted')}>Dismiss</Button>
    </div>)}
    {actions.status && <p className="chat-error" role="alert">{actions.status}</p>}
  </div>;
}

/** All your threads, and a new one you write yourself. */
export function ThreadsDialog({ directory, assistantName, close }: { directory: Directory; assistantName: string; close: () => void }) {
  const { threads, reload } = useThreads();
  const actions = useThreadActions();
  const create = useMutation();
  const [adding, setAdding] = useState(false);
  const [to, setTo] = useState('');
  const [title, setTitle] = useState('');
  const [opening, setOpening] = useState('');
  // One ID per exact thread, so a retried start after a lost answer sends once.
  const attempt = useRef<{ key: string; id: string }>(undefined);
  const me = workspaceSession?.user?.id;
  const agents = directory.agents.filter(agent => !agent.assistant && (agent.audience !== 'only_me' || agent.yours));
  const people = directory.users.filter(user => user.id !== me);
  const ready = Boolean(to && title.trim() && opening.trim());
  const start = () => void create.run(async () => {
    if (!ready) return;
    const [kind, counterpart] = to.split(':');
    const key = JSON.stringify([to, title.trim(), opening.trim()]);
    if (attempt.current?.key !== key) attempt.current = { key, id: crypto.randomUUID() };
    const result = await chatRequest<{ chat: string }>('/assistant/threads', { id: attempt.current.id, kind, counterpart, title: title.trim(), opening: opening.trim() });
    reload(); close(); navigate(chatUrl(result.chat));
  });
  const group = (label: string, state: AssistantThread['state']) => {
    const list = threads.filter(thread => thread.state === state);
    if (!list.length) return null;
    return <section aria-label={label}><h3>{label}</h3>{list.map(thread => <div className="chat-thread-item" key={thread.id}>
      <span><strong>{thread.title}</strong><small>with {withName(thread)}{thread.sent > 0 ? ` · ${thread.sent} sent` : ''}{thread.state === 'stopped' && thread.detail ? ` · ${thread.detail}` : ''}</small>
        {state === 'suggested' && <q>{thread.opening}</q>}</span>
      {thread.chat && <Button onClick={() => { close(); navigate(chatUrl(thread.chat!)); }}>Open</Button>}
      {state === 'running' && <Button disabled={actions.busy} onClick={() => actions.set(thread, 'stopped')}>Stop</Button>}
      {state === 'suggested' && <Button primary disabled={actions.busy} onClick={() => actions.set(thread, 'running')}>Start</Button>}
      {state === 'stopped' && <Button disabled={actions.busy} onClick={() => actions.set(thread, 'running')}>Resume</Button>}
      {state !== 'running' && <Button disabled={actions.busy} onClick={() => actions.set(thread, 'deleted')}>{state === 'suggested' ? 'Dismiss' : 'Delete'}</Button>}
    </div>)}</section>;
  };
  return <Dialog title="Threads" busy={create.busy} status={create.status || actions.status} close={close}
    footer={adding
      ? <><Button onClick={() => setAdding(false)} disabled={create.busy}>Back</Button><Button primary disabled={!ready || create.busy} onClick={start}>{create.busy ? 'Starting…' : 'Start thread'}</Button></>
      : <><Button onClick={close}>Close</Button><Button primary onClick={() => setAdding(true)}>New thread</Button></>}>
    <div className="chat-threads">
      {adding ? <>
        <p className="chat-help">{assistantName} will talk to them for you about this one matter. The first message is sent as yours, and so is every answer, until you stop the thread.</p>
        <label>With<select value={to} onChange={event => setTo(event.target.value)}>
          <option value="">Choose a person or an agent</option>
          <optgroup label="Agents">{agents.map(agent => <option key={agent.id} value={`agent:${agent.id}`}>{agent.name}</option>)}</optgroup>
          <optgroup label="People">{people.map(user => <option key={user.id} value={`user:${user.id}`}>{user.name}</option>)}</optgroup>
        </select></label>
        <label>What it is about<input value={title} maxLength={120} onChange={event => setTitle(event.target.value)} placeholder="For example: Leave in December" /></label>
        <label>First message<textarea value={opening} maxLength={4000} onChange={event => setOpening(event.target.value)} placeholder="Written as you would write it" /></label>
      </> : <>
        <p className="chat-help">A thread lets {assistantName} talk to one agent or person for you about one matter. Its messages appear in your chat with them as yours. Only you and workspace admins can tell which ones {assistantName} wrote.</p>
        {group('Suggested', 'suggested')}{group('Running', 'running')}{group('Stopped', 'stopped')}
        {!threads.length && <p className="chat-empty">No threads yet. Ask {assistantName} to follow something up, or start one yourself.</p>}
      </>}
    </div>
  </Dialog>;
}
