import { useEffect, useRef, useState } from 'react';
import { workspaceSession, workspaceUrl } from '../../workspace-context';
import { AgentFace, AiTag, PersonFace } from '../faces';
import { errorMessage } from '../requests';
import { Button } from '../ui';
import { subscribeChanges } from './live';
import type { Directory } from './model';
import { alertText } from './notifications';
import { changesUnread } from './unread-counts';
import { earlierItems, itemPlace, itemTime, loadForYou, reasonLabel, type ForYouFilter, type ForYouItem, type ForYouPage } from './for-you-model';
import './for-you.css';

const FILTERS: { id: ForYouFilter; label: string }[] = [
  { id: 'all', label: 'All' }, { id: 'mention', label: '@Mentions' }, { id: 'agent_reply', label: 'Agent replies' }, { id: 'thread', label: 'Threads' },
];

/** Chat → For you: every message addressed to the person in channels and groups.
 * It never marks anything read: an item is read when its message is seen in its
 * conversation, so this list, the counts and the rail agree. */
export function ForYouPage({ directory, back }: { directory?: Directory; back: () => void }) {
  const [first, setFirst] = useState<ForYouPage>();
  const [older, setOlder] = useState<ForYouPage[]>([]);
  const [error, setError] = useState('');
  const [filter, setFilter] = useState<ForYouFilter>('all');
  const [loadingOlder, setLoadingOlder] = useState(false);
  const olderBusy = useRef(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    const abort = new AbortController(), viewer = workspaceSession?.user?.id;
    let busy = false, again = false, timer: number | undefined;
    // One read at a time, so an older answer never replaces a newer one.
    const read = async () => {
      if (busy) { again = true; return; }
      busy = true;
      try { const page = await loadForYou(null, abort.signal); if (!abort.signal.aborted) { setFirst(page); setError(''); } }
      catch (e) {
        if (abort.signal.aborted) return;
        setError(errorMessage(e));
        // Denied or removed access clears the list; an outage keeps what was shown.
        if (e instanceof Error && 'status' in e && [400, 401, 403, 404].includes(Number(e.status))) { setFirst(undefined); setOlder([]); }
      } finally { busy = false; if (again && !abort.signal.aborted) { again = false; void read(); } }
    };
    void read();
    // What changes the counts changes this list: new messages, receipts, access.
    const unsubscribe = subscribeChanges(change => {
      if (!changesUnread(change, viewer) || timer !== undefined) return;
      timer = window.setTimeout(() => { timer = undefined; void read(); }, 500);
    });
    return () => { alive.current = false; abort.abort(); unsubscribe(); window.clearTimeout(timer); };
  }, []);
  const cursor = older.length ? older[older.length - 1].next_before : first?.next_before ?? null;
  const loadOlder = async () => {
    if (olderBusy.current || !cursor) return;
    olderBusy.current = true; setLoadingOlder(true);
    try { const page = await loadForYou(cursor); if (alive.current) setOlder(pages => [...pages, page]); }
    catch (e) { if (alive.current) setError(errorMessage(e)); }
    finally { olderBusy.current = false; if (alive.current) setLoadingOlder(false); }
  };
  const fresh = first?.new ?? [];
  const earlier = first ? earlierItems(first, older) : [];
  const shown = (items: ForYouItem[]) => filter === 'all' ? items : items.filter(item => item.reason === filter);
  const count = (id: ForYouFilter) => id === 'all' ? fresh.length : fresh.filter(item => item.reason === id).length;
  const newShown = shown(fresh), earlierShown = shown(earlier);
  return <section className="for-you" aria-label="For you">
    <header className="for-you-header">
      <Button className="chat-back" onClick={back}>‹ Chats</Button>
      <div className="for-you-title">
        <h2>For you</h2>
        <p>@mentions, replies to your requests, and replies in threads you follow</p>
      </div>
      <a className="ui-button for-you-settings" href={workspaceUrl('/?view=settings&section=preferences')}>Notification settings</a>
      <div className="for-you-filters" role="group" aria-label="Show">
        {FILTERS.map(({ id, label }) => <button type="button" key={id} aria-pressed={filter === id} onClick={() => setFilter(id)}>
          {label}{count(id) > 0 && <span aria-label={`, ${count(id)} new`}> · {count(id)}</span>}
        </button>)}
      </div>
    </header>
    <div className="for-you-list">
      {!first && !error && <p className="chat-help" role="status">Loading what is for you…</p>}
      {error && <p className="chat-error" role="alert">{error}</p>}
      {first && !fresh.length && !earlier.length ? <div className="for-you-empty">
        <span aria-hidden="true">✓</span>
        <h3>Nothing for you right now</h3>
        <p>When someone @mentions you, an agent answers your request, or there is a reply in a thread you follow, it shows here.</p>
        <p>Approvals and agent questions are in <a href={workspaceUrl('/?view=activity&tab=needs')}>Activity → Needs me</a>.</p>
      </div> : first && <>
        {newShown.length > 0 && <><h3 className="for-you-section">New · {newShown.length}</h3>
          <ul>{newShown.map(item => <ForYouRow key={item.id} item={item} directory={directory} />)}</ul></>}
        {earlierShown.length > 0 && <><h3 className="for-you-section">Earlier</h3>
          <ul>{earlierShown.map(item => <ForYouRow key={item.id} item={item} directory={directory} />)}</ul></>}
        {!newShown.length && !earlierShown.length && <p className="chat-help">Nothing here for this filter.</p>}
        {cursor && <Button className="for-you-older" disabled={loadingOlder} onClick={() => void loadOlder()}>{loadingOlder ? 'Loading…' : 'Older'}</Button>}
      </>}
    </div>
  </section>;
}

function ForYouRow({ item, directory }: { item: ForYouItem; directory?: Directory }) {
  const agent = item.kind === 'agent' ? directory?.agents.find(entry => entry.id === item.author) : undefined;
  const person = item.kind === 'agent' ? undefined : directory?.users.find(entry => entry.id === item.author);
  const name = agent?.name ?? person?.name ?? item.name ?? (item.kind === 'agent' ? 'Agent' : 'Former member');
  const words = item.unavailable ? 'Response unavailable because source access changed.' : alertText(item.content) || 'Sent a file';
  // Opens the conversation at the message; a thread reply opens its thread.
  const href = workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(item.chat)}&message=${encodeURIComponent(item.id)}&from=for-you`);
  return <li><a className={`for-you-item${item.unread ? ' for-you-item-new' : ''}`} href={href}>
    {item.kind === 'agent' ? <AgentFace agent={agent ?? { id: item.author, name }} /> : <PersonFace person={person ?? { id: item.author, name }} />}
    <span className="for-you-body">
      <span className="for-you-meta">
        {item.unread && <span className="visually-hidden">New: </span>}
        <strong>{name}</strong>{item.kind === 'agent' && <>{' '}<AiTag /></>}{' '}
        <span className={`for-you-reason for-you-reason-${item.reason}`}>{reasonLabel[item.reason]}</span>{' '}
        <span className="for-you-where">{itemPlace(item)} · {itemTime(item.at)}</span>
      </span>{' '}
      <span className={`for-you-words${item.unavailable ? ' for-you-unavailable' : ''}`}>{words}</span>
    </span>
    {item.unread && <span className="for-you-dot" aria-hidden="true" />}
  </a></li>;
}
