import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { AgentFace } from '../faces';
import { useMutation } from '../requests';
import { QuestionDialog } from '../questions/dialog';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { useAttention } from '../attention/use-attention';
import { acknowledgeAttention, attentionKey, type AttentionItem } from '../attention/model';
import { ago } from './model';
import { RequestDetails } from './request-details';
import { ReviewTogether } from './review-together';

type Attention = ReturnType<typeof useAttention>;
const clean = (title: string) => title.replace(/^\[Ayati request mode:[^\]]*\]\s*/, '');
/** Most urgent first: approvals an admin may already step in on, then the longest waiting. */
function urgent(a: AttentionItem, b: AttentionItem, now = Date.now()) {
  const late = (i: AttentionItem) => (i.backup_at && i.backup_at <= now ? 1 : 0);
  return late(b) - late(a) || (a.created_at ?? a.at) - (b.created_at ?? b.at);
}
/** Similar approvals (same agent, same action) fold into one row. */
function folded(list: AttentionItem[]): (AttentionItem | AttentionItem[])[] {
  const out: (AttentionItem | AttentionItem[])[] = [];
  for (const item of list) {
    const key = item.operation ? `${item.agent}/${item.connection}/${item.operation}` : undefined;
    const group = key ? out.find(entry => Array.isArray(entry) && `${entry[0].agent}/${entry[0].connection}/${entry[0].operation}` === key) as AttentionItem[] | undefined : undefined;
    if (group) group.push(item);
    else if (key && list.filter(other => `${other.agent}/${other.connection}/${other.operation}` === key).length > 1) out.push([item]);
    else out.push(item);
  }
  return out;
}
const waitLine = (item: AttentionItem, now = Date.now()) => item.backup_at && item.backup_at <= now ? ' · an admin can decide now'
  : item.backup_at ? ` · admin backup in ${Math.max(1, Math.round((item.backup_at - now) / 3600000))} h` : '';
const action = (item: AttentionItem) => item.kind === 'question' ? 'Answer' : item.kind === 'approval' ? item.status === 'uncertain' ? 'Check' : 'Review' : 'Open';

/** Needs me: approvals, questions and work that needs a look, from every agent
 * (designs/activity 1). Opening an item records a read receipt first; reading
 * never answers or approves anything. `first` is the page's own read, which also
 * gives the tab its count. */
export function NeedsMe({ first, search }: { first: Attention; search: string }) {
  const [cursors, setCursors] = useState<string[]>([]);
  const older = useAttention('needs_you', cursors.at(-1) ?? null, cursors.length > 0);
  const { data, error, loading, refresh } = cursors.length ? older : first;
  const [open, setOpen] = useState<AttentionItem>();
  const [together, setTogether] = useState<AttentionItem[]>();
  const mutation = useMutation();
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  // Closing a dialog returns keyboard focus to the button that opened it.
  const opener = useRef<HTMLElement | null>(null), restore = useRef(false);
  useEffect(() => {
    if (open || !restore.current) return;
    restore.current = false;
    if (opener.current?.isConnected) opener.current.focus();
  }, [open]);
  const select = (item: AttentionItem) => void mutation.run(async () => {
    opener.current = document.activeElement as HTMLElement | null;
    try { await acknowledgeAttention(item); if (alive.current) setOpen(item); }
    catch (failure) { await refresh(); throw failure; }
  });
  const reviewed = (item: AttentionItem) => void mutation.run(async () => {
    try { await acknowledgeAttention(item, true); await refresh(); }
    catch (failure) { await refresh(); throw failure; }
  });
  const close = () => { restore.current = true; setOpen(undefined); void refresh(); };
  const items = (data?.items ?? []).filter(item => !search || `${item.title} ${item.agent_name} ${item.conversation_name}`.toLowerCase().includes(search.toLowerCase()));
  const groups: [string, AttentionItem[]][] = [
    ['Approvals', items.filter(i => i.kind === 'approval' && i.status !== 'uncertain').sort((a, b) => urgent(a, b))],
    ['Questions', items.filter(i => i.kind === 'question').sort((a, b) => urgent(a, b))],
    ['Needs review', items.filter(i => i.kind === 'outcome' || i.status === 'uncertain')],
  ];
  return <section className="activity-needs" aria-label="Needs me">
    {error && <p role="alert">{error} <Button disabled={loading} onClick={() => void refresh()}>Retry</Button></p>}
    {mutation.status && <p role="alert">{mutation.status}</p>}
    {!data && !error && <p role="status">Checking what needs you…</p>}
    {data?.available === false && <p className="activity-empty">Shared-work notifications are unavailable in this workspace.</p>}
    {data?.available && !items.length && <div className="activity-empty"><strong>Nothing needs you right now</strong>
      <p>Approvals, questions and work that needs a look appear here.</p></div>}
    {groups.filter(([, list]) => list.length).map(([title, list]) => <section key={title} aria-label={title}>
      <h2 className="activity-group">{title} · {list.length}</h2>
      <ul className="activity-list">{folded(list).map(entry => Array.isArray(entry) ? <li key={`group.${attentionKey(entry[0])}`} className="activity-folded">
        <span className="activity-mark approval" aria-hidden="true">◇</span>
        <span className="activity-main"><strong>{entry.length} × {entry[0].operation_title ?? clean(entry[0].title)}</strong>
          <small><AgentFace agent={{ id: entry[0].agent, name: entry[0].agent_name }} className="activity-face" />{entry[0].agent_name} · oldest {ago(Math.min(...entry.map(i => i.created_at ?? i.at)))}{waitLine(entry[0])}</small></span>
        <Button primary disabled={mutation.busy} onClick={() => setTogether(entry)}>Review together</Button>
      </li> : (item => <li key={attentionKey(item)} className={open && attentionKey(open) === attentionKey(item) ? 'selected' : undefined}>
        <span className={`activity-mark ${item.kind}`} aria-hidden="true">{item.kind === 'question' ? '?' : item.kind === 'approval' && item.status !== 'uncertain' ? '◇' : '!'}</span>
        <span className="activity-main"><strong>{clean(item.title)}</strong>
          <small><AgentFace agent={{ id: item.agent, name: item.agent_name }} className="activity-face" />{item.agent_name} · {item.conversation_name} · {ago(item.created_at ?? item.at)}{item.read ? '' : ' · new'}{item.kind === 'approval' ? waitLine(item) : ''}</small>
          {item.status === 'uncertain' && <small>Check the business system. This entry cannot repeat or confirm the action.</small>}</span>
        {item.kind === 'outcome' && item.status !== 'complete' && !item.reviewed
          && <Button disabled={mutation.busy} onClick={() => reviewed(item)}>Mark reviewed</Button>}
        <Button primary={item.kind === 'approval' && item.status !== 'uncertain'} disabled={mutation.busy} onClick={() => select(item)}>{action(item)}</Button>
      </li>)(entry))}</ul>
    </section>)}
    {(cursors.length > 0 || data?.next_before) && <nav className="activity-pages" aria-label="Needs me pages">
      {cursors.length > 0 && <Button disabled={loading} onClick={() => setCursors(old => old.slice(0, -1))}>Newer</Button>}
      {data?.next_before && <Button disabled={loading} onClick={() => setCursors(old => [...old, data.next_before as string])}>Older</Button>}
    </nav>}
    {together && <ReviewTogether items={together} close={() => { setTogether(undefined); void refresh(); }} />}
    {open?.kind === 'approval' && <ApprovalDialog key={open.id} id={open.id} close={close} />}
    {open?.kind === 'question' && <QuestionDialog key={open.id} id={open.id} close={close} />}
    {open?.kind === 'outcome' && <RequestDetails key={open.id} reply={open.reply} close={close} />}
  </section>;
}
