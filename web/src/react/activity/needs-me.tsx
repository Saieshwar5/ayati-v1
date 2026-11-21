import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Button, DialogPlacement } from '../ui';
import { AgentFace } from '../faces';
import { useMutation, useResource } from '../requests';
import { chatRequest, type Conversation } from '../chat/model';
import { QuestionDialog } from '../questions/dialog';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { OutcomeDialog } from '../approvals/outcome-dialog';
import { useAttention } from '../attention/use-attention';
import { acknowledgeAttention, attentionKey, type AttentionItem } from '../attention/model';
import { ago, loadNow, type FixGroup } from './model';
import { useLive } from './use-live';
import { FixDetails, fixGroups, fixKey } from './fixing';
import { RequestTalk } from './request-talk';
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
const late = (item: AttentionItem, now = Date.now()) => Boolean(item.backup_at && item.backup_at <= now);
const glyph = (item: AttentionItem) => item.kind === 'question' ? '?' : item.kind === 'approval' && item.status !== 'uncertain' ? '◇' : '!';
/** One row of the list: a single item, similar approvals folded together, or a cause to fix. */
type Entry = AttentionItem | AttentionItem[] | FixGroup;
const isFix = (entry: Entry): entry is FixGroup => !Array.isArray(entry) && 'cause' in entry;
const entryKey = (entry: Entry) => isFix(entry) ? fixKey(entry) : Array.isArray(entry) ? `group.${attentionKey(entry[0])}` : attentionKey(entry);
const entryTitle = (entry: Entry) => isFix(entry) ? entry.title : Array.isArray(entry) ? `${entry.length} × ${entry[0].operation_title ?? clean(entry[0].title)}` : clean(entry.title);
const loadChats = () => chatRequest<{ chats: Conversation[] }>('/chats');
/** The open row is kept in the address, so a reload or the way back from Chat finds it. */
function remember(key?: string) {
  const url = new URL(window.location.href);
  if (key) url.searchParams.set('item', key); else url.searchParams.delete('item');
  window.history.replaceState(null, '', url);
}
const action = (item: AttentionItem) => item.kind === 'question' ? item.status === 'browser' ? 'Take over' : 'Answer' : item.kind === 'approval' ? item.status === 'uncertain' ? 'Check' : 'Review' : 'Open';

/** Needs me: approvals, questions and work that needs a look, from every agent
 * (designs/activity 1). Opening an item records a read receipt first; reading
 * never answers or approves anything. `first` is the page's own read, which also
 * gives the tab its count. With `pane` (a wide window) the opened item shows beside
 * the list instead of over it (designs/activity-next, Design A): rows are the
 * buttons, J and K move through them, and the list is re-read after each decision.
 * What needs fixing on the agents this person manages is listed here too; it is not
 * part of the count. `initial` is the row a link asked for. */
export function NeedsMe({ first, search, pane = false, initial }: { first: Attention; search: string; pane?: boolean; initial?: string }) {
  const [cursors, setCursors] = useState<string[]>([]);
  const older = useAttention('needs_you', cursors.at(-1) ?? null, cursors.length > 0);
  const { data, error, loading, refresh } = cursors.length ? older : first;
  const [open, setOpen] = useState<AttentionItem>();
  const [together, setTogether] = useState<AttentionItem[]>();
  const [fix, setFix] = useState<FixGroup>();
  const now = useLive(useCallback(() => loadNow('mine'), []));
  // Only the reading pane shows a conversation, and only to someone who is in it.
  const chats = useResource(useCallback(() => pane ? loadChats() : Promise.resolve(undefined), [pane]));
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
    try { await acknowledgeAttention(item); if (alive.current) { setTogether(undefined); setFix(undefined); setOpen(item); remember(attentionKey(item)); } }
    catch (failure) { await refresh(); throw failure; }
  });
  const reviewed = (item: AttentionItem) => void mutation.run(async () => {
    try { await acknowledgeAttention(item, true); await refresh(); }
    catch (failure) { await refresh(); throw failure; }
  });
  const close = () => { restore.current = true; setOpen(undefined); remember(); void refresh(); };
  const activate = (entry: Entry) => {
    if (isFix(entry)) { setOpen(undefined); setTogether(undefined); setFix(entry); remember(fixKey(entry)); }
    else if (Array.isArray(entry)) { setOpen(undefined); setFix(undefined); setTogether(entry); remember(entryKey(entry)); }
    else select(entry);
  };
  const matches = (text: string) => !search || text.toLowerCase().includes(search.toLowerCase());
  const fixes = fixGroups(now.data?.fixing).filter(group => matches(`${group.title} ${group.agents.map(agent => agent.name).join(' ')}`));
  const items = (data?.items ?? []).filter(item => matches(`${item.title} ${item.agent_name} ${item.conversation_name}`));
  const groups: [string, AttentionItem[]][] = [
    ['Approvals', items.filter(i => i.kind === 'approval' && i.status !== 'uncertain').sort((a, b) => urgent(a, b))],
    // A browser hand-over waits for the person to use the agent's browser (designs/agent-computers 11).
    ['In the browser', items.filter(i => i.kind === 'question' && i.status === 'browser').sort((a, b) => urgent(a, b))],
    ['Questions', items.filter(i => i.kind === 'question' && i.status !== 'browser').sort((a, b) => urgent(a, b))],
    ['Needs review', items.filter(i => i.kind === 'outcome' || i.status === 'uncertain')],
  ];
  const sections: (readonly [string, number, Entry[]])[] = groups.filter(([, list]) => list.length).map(([title, list]) => [title, list.length, folded(list)] as const);
  if (fixes.length) sections.push(['Needs fixing', fixes.length, fixes]);
  const entries = sections.flatMap(([, , rows]) => rows);
  const shown = fix ? fixKey(fix) : together ? entryKey(together) : open ? attentionKey(open) : undefined;
  const listed = entries.some(entry => entryKey(entry) === shown);

  // J and K (and the arrow keys inside the list) open the next and previous row.
  const list = useRef<HTMLDivElement>(null);
  const move = useRef<(step: number) => void>(() => {});
  move.current = step => {
    if (!entries.length) return;
    const next = entries[Math.min(entries.length - 1, Math.max(0, entries.findIndex(entry => entryKey(entry) === shown) + step))];
    if (entryKey(next) === shown) return;
    [...list.current?.querySelectorAll<HTMLElement>('[data-entry]') ?? []].find(node => node.dataset.entry === entryKey(next))?.focus();
    activate(next);
  };
  useEffect(() => {
    if (!pane) return;
    const keys = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || event.defaultPrevented) return;
      const target = event.target as HTMLElement | null;
      if (target?.closest('input, textarea, select, [contenteditable]') || document.querySelector('dialog[open]')) return;
      const inList = Boolean(target?.closest('.activity-needs-list'));
      // A, R and C press the opened approval's own buttons, only while the keyboard is in this list or pane.
      const decide = ['a', 'r', 'c'].includes(event.key) && !event.repeat && target?.closest('.activity-needs')
        ? list.current?.parentElement?.querySelector<HTMLButtonElement>(`.activity-pane [data-shortcut="${event.key}"]:not(:disabled)`) : undefined;
      if (decide) { event.preventDefault(); decide.click(); return; }
      const step = event.key === 'j' || (inList && event.key === 'ArrowDown') ? 1 : event.key === 'k' || (inList && event.key === 'ArrowUp') ? -1 : 0;
      if (!step) return;
      event.preventDefault(); move.current(step);
    };
    document.addEventListener('keydown', keys);
    return () => document.removeEventListener('keydown', keys);
  }, [pane]);
  // A link to one row (the way back from Chat, or an agent's "Needs you") finds it again.
  // It is opened only if this person has read it before: opening saves a receipt.
  const asked = useRef(initial);
  useEffect(() => {
    if (!asked.current || !data) return;
    const entry = entries.find(candidate => entryKey(candidate) === asked.current);
    if (!entry && now.data === undefined) return;
    asked.current = undefined;
    if (!entry) return;
    [...list.current?.querySelectorAll<HTMLElement>('[data-entry]') ?? []].find(node => node.dataset.entry === entryKey(entry))?.focus();
    if (isFix(entry)) setFix(entry); else if (Array.isArray(entry)) setTogether(entry); else if (entry.read) setOpen(entry);
  });
  const talk = pane && open && open.kind !== 'outcome' && open.status !== 'uncertain' ? open : undefined;
  const member = Boolean(chats.data?.chats?.some(chat => chat.id === talk?.chat));
  const reread = now.refresh;
  const placement = useMemo(() => pane ? { settled: () => void refresh(),
    after: talk && chats.data ? <RequestTalk key={attentionKey(talk)} item={talk} member={member} /> : undefined } : undefined, [pane, refresh, talk, member, chats.data]);

  const row = (entry: Entry) => {
    const key = entryKey(entry), selected = key === shown;
    if (isFix(entry)) {
      const main = <><span className="activity-mark outcome" aria-hidden="true">!</span>
        <span className="activity-main"><strong>{entry.title}</strong>
          <small>{entry.agents.map(agent => agent.name).join(', ')}{entry.items[0]?.detail && entry.cause !== 'failing' ? ` · ${entry.items[0].detail}` : ''}</small></span></>;
      return pane ? <li key={key} className={selected ? 'selected' : undefined}>
        <button type="button" className="activity-need-row" data-entry={key} aria-current={selected ? 'true' : undefined} onClick={() => activate(entry)}>
          {main}<span className="activity-need-action">Fix</span></button></li>
        : <li key={key} className={selected ? 'selected' : undefined}>{main}<Button data-entry={key} onClick={() => activate(entry)}>See the fix</Button></li>;
    }
    const item = Array.isArray(entry) ? entry[0] : entry, group = Array.isArray(entry) ? entry : undefined;
    const label = group ? 'Review together' : action(item);
    const waiting = group || item.kind === 'approval' ? waitLine(item) : '';
    const main = <>
      <span className={`activity-mark ${group ? 'approval' : item.kind}`} aria-hidden="true">{group ? '◇' : glyph(item)}</span>
      <span className="activity-main"><strong>{entryTitle(entry)}</strong>
        <small>{pane && late(item) && <span className="activity-status waiting">An admin can decide now</span>}
          <AgentFace agent={{ id: item.agent, name: item.agent_name }} className="activity-face" />{item.agent_name}
          {group ? ` · oldest ${ago(Math.min(...group.map(i => i.created_at ?? i.at)))}` : ` · ${item.conversation_name} · ${ago(item.created_at ?? item.at)}${item.read ? '' : ' · new'}`}
          {pane && late(item) ? '' : waiting}</small>
        {!group && item.status === 'uncertain' && <small>The app didn’t confirm it, so its writes are paused. Check whether it went out.</small>}</span></>;
    const mark = !group && item.kind === 'outcome' && item.status !== 'complete' && !item.reviewed
      && <Button disabled={mutation.busy} onClick={() => reviewed(item)}>Mark reviewed</Button>;
    const state = [group && 'activity-folded', selected && 'selected', !group && !item.read && 'unread'].filter(Boolean).join(' ') || undefined;
    if (pane) return <li key={key} className={state}>
      <button type="button" className="activity-need-row" data-entry={key} aria-current={selected ? 'true' : undefined} onClick={() => activate(entry)}>
        {main}<span className="activity-need-action">{label}</span></button></li>;
    return <li key={key} className={state}>{main}{mark}
      <Button primary={Boolean(group) || (item.kind === 'approval' && item.status !== 'uncertain')} disabled={mutation.busy} onClick={() => activate(entry)}>{label}</Button></li>;
  };
  // In the pane a failed outcome is cleared from beside its details, not from its row.
  const current = open && items.find(item => attentionKey(item) === attentionKey(open));
  const clearable = current?.kind === 'outcome' && current.status !== 'complete' && !current.reviewed ? current : undefined;
  const details = fix ? <FixDetails key={fixKey(fix)} group={fix} close={() => { setFix(undefined); remember(); void reread(); }} changed={() => void reread().catch(() => {})} />
    : together ? <ReviewTogether items={together} close={() => { setTogether(undefined); remember(); void refresh(); }} />
    : open?.kind === 'approval' && open.status !== 'uncertain' ? <ApprovalDialog key={open.id} id={open.id} close={close} />
    : open?.kind === 'approval' ? <OutcomeDialog key={open.id} id={open.id} close={close} />
    : open?.kind === 'question' ? <QuestionDialog key={open.id} id={open.id} close={close} />
    : open?.kind === 'outcome' ? <RequestDetails key={open.id} reply={open.reply} close={close} /> : null;
  return <section className="activity-needs activity-cols" aria-label="Needs you">
    <div className="activity-needs-list activity-cols-list" ref={list}>
      {error && <p role="alert">{error} <Button disabled={loading} onClick={() => void refresh()}>Retry</Button></p>}
      {mutation.status && <p role="alert">{mutation.status}</p>}
      {!data && !error && <p role="status">Checking what needs you…</p>}
      {data?.available === false && <p className="activity-empty">Shared-work notifications are unavailable in this workspace.</p>}
      {data?.available && !items.length && !fixes.length && <div className="activity-empty"><strong>Nothing needs you right now</strong>
        <p>Approvals, questions and work that needs a look appear here.</p></div>}
      {sections.map(([title, count, rows]) => <section key={title} aria-label={title}>
        <h2 className="activity-group">{title} · {count}</h2>
        <ul className="activity-list">{rows.map(row)}</ul>
      </section>)}
      {(cursors.length > 0 || data?.next_before) && <nav className="activity-pages" aria-label="Needs you pages">
        {cursors.length > 0 && <Button disabled={loading} onClick={() => setCursors(old => old.slice(0, -1))}>Newer</Button>}
        {data?.next_before && <Button disabled={loading} onClick={() => setCursors(old => [...old, data.next_before as string])}>Older</Button>}
      </nav>}
    </div>
    <DialogPlacement.Provider value={placement}><div className="activity-pane">
      {details}
      {pane && !details && entries.length > 0 && <p className="activity-empty">Choose something on the left to read it here.<br />
        <kbd>J</kbd> <kbd>K</kbd> move through the list. On an approval, <kbd>A</kbd> approves, <kbd>R</kbd> rejects and <kbd>C</kbd> requests changes.</p>}
      {pane && clearable && <p className="activity-pane-next">
        <Button disabled={mutation.busy} onClick={() => reviewed(clearable)}>Mark reviewed</Button>
        <small>Clears it from this list. Nothing is retried or changed.</small></p>}
      {pane && details && !listed && entries.length > 0 && <p className="activity-pane-next">
        <Button primary disabled={mutation.busy} onClick={() => activate(entries[0])}>Next: {entryTitle(entries[0])}</Button></p>}
    </div></DialogPlacement.Provider>
  </section>;
}
