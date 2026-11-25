import { useCallback, useEffect, useMemo, useState } from 'react';
import { Button, DialogPlacement } from '../ui';
import { useMutation, useResource } from '../requests';
import { AgentFace } from '../faces';
import { loadAgents } from '../agents/model';
import { loadMembers } from '../agents/create/people-step';
import { chatRequest, type Conversation } from '../chat/model';
import { clock, dayLabel, exportUrl, foldPanelUpdates, kindLabels, taskLabels, loadChanges, loadHistory, loadTotals, startOfToday, status, where, type ActivityItem, type Filters, type Scope, type Totals } from './model';
import { RequestDetails } from './request-details';
import { UnhandledEvents } from './unhandled';

const periods = [['today', 'Today'], ['7', 'Last 7 days'], ['30', 'Last 30 days'], ['90', 'Last 90 days']] as const;
const statuses = [['', 'Any status'], ['active', 'In progress'], ['waiting', 'Waiting'], ['done', 'Done'], ['review', 'Needs review'], ['failed', 'Could not complete'], ['stopped', 'Stopped']] as const;
const kinds = [['', 'Any type'], ['chat', 'Chat'], ['task', 'Task runs'], ['trigger', 'Triggers'], ['handoff', 'Hand-offs'], ['panel', 'Panel updates'], ['unhandled', 'Events not handled'], ['changes', 'Setting changes']] as const;
const fromPeriod = (period: string) => period === 'today' ? startOfToday() : startOfToday() - (Number(period) - 1) * 86400000;
const loadChats = () => chatRequest<{ chats: Conversation[] }>('/chats');
/** The search text once typing pauses, so each keystroke doesn't start a read. */
function useSettled(text: string): string {
  const [settled, setSettled] = useState(text);
  useEffect(() => { const timer = window.setTimeout(() => setSettled(text), 300); return () => window.clearTimeout(timer); }, [text]);
  return settled;
}

/** History: a timeline of everything agents did, with filters in one row and the
 * period's totals as one line above it (designs/activity 3; the log first and a row
 * opening beside it from designs/activity-next, Design A). The charts are on
 * Insights. An agent's Activity tab is this view for one agent. */
export function HistoryView({ agent, search: outer, since, scope, initialKind, initialAgent, pane = false }: {
  agent?: string; search?: string; since?: number; scope?: Scope; initialKind?: string;
  /** The agent the filter starts on, when a link asked for one. */
  initialAgent?: string; pane?: boolean }) {
  // An agent's tab has no page search above it, so it brings its own.
  const [own, setOwn] = useState('');
  const search = useSettled(outer ?? own);
  const [period, setPeriod] = useState(since ? 'since' : '7');
  const [filters, setFilters] = useState<Filters>({ agent: agent ?? initialAgent, kind: initialKind });
  // Setting changes and unhandled events aren't requests: no status filter or export.
  const special = filters.kind === 'changes' || filters.kind === 'unhandled';
  const from = period === 'since' && since ? since : fromPeriod(period);
  const query: Filters = { ...filters, agent: agent ?? filters.agent, from, q: search.trim() || undefined, scope };
  const agents = useResource(loadAgents);
  const members = useResource(loadMembers);
  const chats = useResource(loadChats);
  const places = (chats.data?.chats ?? []).filter(chat => chat.kind !== 'direct');
  const set = (key: keyof Filters, value: string) => setFilters(old => ({ ...old, [key]: value || undefined }));
  const chips = [
    !agent && filters.agent && { key: 'agent' as const, label: `Agent: ${agents.data?.agents.find(a => a.id === filters.agent)?.name ?? 'Unknown'}` },
    filters.person && { key: 'person' as const, label: `Person: ${members.data?.users.find(u => u.id === filters.person)?.name ?? 'Unknown'}` },
    filters.chat && { key: 'chat' as const, label: `Where: #${places.find(c => c.id === filters.chat)?.name ?? 'Unknown'}` },
    filters.status && { key: 'status' as const, label: `Status: ${statuses.find(s => s[0] === filters.status)?.[1]}` },
    filters.kind && { key: 'kind' as const, label: `Type: ${kinds.find(k => k[0] === filters.kind)?.[1]}` },
  ].filter(Boolean) as { key: keyof Filters; label: string }[];
  return <section className="activity-history" aria-label="History">
    <div className="activity-filters" role="group" aria-label="Filters">
      {outer === undefined && <input type="search" className="activity-search" aria-label="Search activity" placeholder="Search activity"
        value={own} onChange={event => setOwn(event.target.value)} />}
      <label>Period<select value={since ? 'since' : period} onChange={event => setPeriod(event.target.value)}>
        {since && <option value="since">Since you were away</option>}
        {periods.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      {!agent && <label>Agent<select value={filters.agent ?? ''} onChange={event => set('agent', event.target.value)}>
        <option value="">Any agent</option>{agents.data?.agents.map(a => <option key={a.id} value={a.id}>{a.name}</option>)}</select></label>}
      <label>Person<select value={filters.person ?? ''} disabled={filters.kind === 'unhandled'} onChange={event => set('person', event.target.value)}>
        <option value="">Anyone</option>{members.data?.users.map(u => <option key={u.id} value={u.id}>{u.name}</option>)}</select></label>
      <label>Where<select value={filters.chat ?? ''} disabled={filters.kind === 'unhandled'} onChange={event => set('chat', event.target.value)}>
        <option value="">Anywhere</option>{places.map(c => <option key={c.id} value={c.id}>#{c.name}</option>)}</select></label>
      <label>Type<select value={filters.kind ?? ''} onChange={event => set('kind', event.target.value)}>
        {kinds.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      <label>Status<select value={filters.status ?? ''} disabled={special} onChange={event => set('status', event.target.value)}>
        {statuses.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      {!special && <a className="ui-button activity-export" href={exportUrl(query)} download="cuckoo-activity.csv">Export CSV</a>}
    </div>
    {chips.length > 0 && <div className="activity-chips">{chips.map(chip => <Button key={chip.key} className="activity-chip" aria-label={`Remove ${chip.label}`}
      onClick={() => set(chip.key, '')}>{chip.label} ×</Button>)}
      <Button className="activity-clear" onClick={() => setFilters({ agent })}>Clear filters</Button></div>}
    {filters.kind === 'changes'
      ? <Changes key={agent ?? filters.agent ?? ''} agent={agent ?? filters.agent} />
      : filters.kind === 'unhandled'
        ? <UnhandledEvents key={JSON.stringify(query)} agent={query.agent} from={query.from} scope={scope} />
        : <Results key={JSON.stringify(query)} query={query} pane={pane} needLook={() => set('status', 'review')} />}
  </section>;
}

/** Totals and the timeline for one set of filters. It is keyed by those filters,
 * so a change starts fresh and late pages for old filters are dropped. */
function Results({ query, pane, needLook }: { query: Filters; pane: boolean; needLook: () => void }) {
  const [filters] = useState(query);
  const totals = useResource(useCallback(() => loadTotals(filters), [filters]));
  const first = useResource(useCallback(() => loadHistory(filters), [filters]));
  const [older, setOlder] = useState<{ items: ActivityItem[]; next: number | null }>();
  const [open, setOpen] = useState<string>();
  const more = useMutation();
  const items = [...(first.data?.items ?? []), ...(older?.items ?? [])];
  const next = older ? older.next : first.data?.next_before ?? null;
  const loadOlder = (before: number) => void more.run(async () => {
    const page = await loadHistory(filters, before);
    setOlder(old => ({ items: [...(old?.items ?? []), ...page.items], next: page.next_before }));
  });
  const [expanded, setExpanded] = useState<string[]>([]);
  const toggle = (key: string) => setExpanded(old => old.includes(key) ? old.filter(k => k !== key) : [...old, key]);
  const placement = useMemo(() => pane ? {} : undefined, [pane]);
  let day = '';
  return <>
    {totals.data && <Summary totals={totals.data} needLook={needLook} />}
    <div className="activity-cols activity-cols-wide"><div className="activity-timeline activity-cols-list">
      {first.error && <p role="alert">{first.error}</p>}
      {!first.data && !first.error && <p role="status">Loading history…</p>}
      {first.data && !items.length && <p className="activity-empty">No agent work matches these filters.</p>}
      <ul className="activity-list">{foldPanelUpdates(items).map(entry => {
        const first = entry.kind === 'item' ? entry.item : entry.items[0];
        const label = dayLabel(first.created_ms); const heading = label !== day; day = label;
        if (entry.kind === 'item') return <li key={first.id} className="activity-row">
          {heading && <h3 className="activity-day">{label}</h3>}<Row item={first} open={setOpen} /></li>;
        const shown = expanded.includes(entry.key);
        return <li key={entry.key} className="activity-row">
          {heading && <h3 className="activity-day">{label}</h3>}
          <button type="button" className="activity-row-button" aria-expanded={shown} onClick={() => toggle(entry.key)}>
            <time>{clock(first.created_ms)}</time>
            <span className="activity-agent"><AgentFace agent={{ id: first.agent, name: first.agent_name, avatar: first.agent_avatar }} className="activity-face" />{first.agent_name}</span>
            <span className="activity-main"><strong>{first.summary}</strong><small>{kindLabels.panel} · updated {entry.items.length} times</small></span>
            <span className="activity-status quiet">{shown ? 'Hide' : 'Show all'}</span>
          </button>
          {shown && <ul className="activity-list activity-fold">{entry.items.map(item => <li key={item.id} className="activity-row"><Row item={item} open={setOpen} /></li>)}</ul>}
        </li>;
      })}</ul>
      {next && <Button disabled={more.busy} onClick={() => loadOlder(next)}>{more.busy ? 'Loading…' : 'Show older'}</Button>}
      {more.status && <p role="alert">{more.status}</p>}
    </div>
    <DialogPlacement.Provider value={placement}><div className="activity-pane">
      {open && <RequestDetails key={open} reply={open} close={() => setOpen(undefined)} />}
      {pane && !open && items.length > 0 && <p className="activity-empty">Choose a request to see every step it took, how each was allowed, and the reply.</p>}
    </div></DialogPlacement.Provider></div>
  </>;
}

function Row({ item, open }: { item: ActivityItem; open: (id: string) => void }) {
  const shown = status(item);
  // A conversation turn that isn't part of any task reads lighter than real work.
  const light = item.kind === 'chat' && !item.task;
  // A task run's or trigger's place already starts with its kind ("Task · Sales summary").
  const place = where(item);
  const about = item.kind === 'chat' ? `${item.requester_name ?? 'Someone'} · ${place}`
    : item.kind === 'task' || item.kind === 'trigger' ? place[0].toUpperCase() + place.slice(1) : `${kindLabels[item.kind]} · ${place}`;
  return <button type="button" className={`activity-row-button${light ? ' activity-row-light' : ''}`} onClick={() => open(item.id)}>
    <time>{clock(item.created_ms)}</time>
    <span className="activity-agent"><AgentFace agent={{ id: item.agent, name: item.agent_name, avatar: item.agent_avatar }} className="activity-face" />{item.agent_name}</span>
    <span className="activity-main"><strong>{item.kind === 'chat' ? `“${item.summary}”` : item.summary}</strong>
      <small>{about}</small>
      {item.approvals.map(a => <small key={a.id} className="activity-action">→ {a.title} · {a.state === 'completed' ? 'done' : a.state}</small>)}
      {item.task && item.task.kind === 'commitment' && <small className="activity-action">Task: {item.task.title} · {taskLabels[item.task.status]}</small>}</span>
    <span className={`activity-status ${shown.tone}`}>{shown.label}</span>
  </button>;
}

/** The period in one line. "Need a look" is the one number worth a click: it filters the log. */
function Summary({ totals, needLook }: { totals: Totals; needLook: () => void }) {
  return <p className="activity-summary" aria-label="Totals">
    <strong>{totals.requests} {totals.requests === 1 ? 'request' : 'requests'}</strong> · {totals.done} done ·{' '}
    {totals.review > 0 ? <button type="button" className="activity-summary-link" onClick={needLook}>{totals.review} need a look</button> : 'none need a look'}
    {' '}· {totals.automatic + totals.approved} actions ({totals.automatic} automatic, {totals.approved} approved) · {totals.task_runs} task runs
    {totals.waiting ? ` · ${totals.waiting} waiting now` : ''} · {totals.people} {totals.people === 1 ? 'person' : 'people'} in {totals.channels} {totals.channels === 1 ? 'channel' : 'channels'}</p>;
}

function Changes({ agent }: { agent?: string }) {
  const { data, error } = useResource(useCallback(() => loadChanges(agent), [agent]));
  return <div className="activity-timeline">
    {error && <p role="alert">{error}</p>}
    {data && !data.changes.length && <p className="activity-empty">No setting changes yet.</p>}
    <ul className="activity-list">{data?.changes.map(change => <li key={change.id} className="activity-row"><div className="activity-row-button">
      <time>{dayLabel(change.at * 1000)} {clock(change.at * 1000)}</time>
      <span className="activity-agent"><AgentFace agent={{ id: change.agent, name: change.agent_name, avatar: change.agent_avatar }} className="activity-face" />{change.agent_name}</span>
      <span className="activity-main"><strong>{change.summary}</strong></span>
    </div></li>)}</ul>
  </div>;
}
