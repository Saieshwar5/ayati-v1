import { useRef, useState } from 'react';
import { useAttention } from '../attention/use-attention';
import { currentWorkspace } from '../../workspace-context';
import { NeedsMe } from './needs-me';
import { NowView } from './now';
import { HistoryView } from './history';
import { RequestDetails } from './request-details';
import { OpenTasks } from './open-tasks';
import { ReviewView } from './review';
import { UpcomingView } from './upcoming';
import { PauseAgentsControl } from './agents-pause';
import { saveScope, savedScope, type Scope } from './model';
import { useMediaQuery } from '../use-media-query';
import './activity.css';

type Tab = 'needs' | 'work' | 'planned' | 'history' | 'insights';
const tabs: [Tab, string, string][] = [['needs', 'Needs you', 'Search what needs you'], ['work', 'Work', 'Search agents and work'],
  ['planned', 'Planned', 'Search planned work'], ['history', 'History', 'Search history'], ['insights', 'Insights', 'Search']];
// Links written before Design A (designs/activity-next) named six tabs; they still open the right place.
const earlier: Record<string, Tab> = { now: 'work', open: 'work', upcoming: 'planned', review: 'insights' };
const chosen = (href: string): Tab | undefined => {
  const tab = new URL(href).searchParams.get('tab') ?? '';
  return tabs.some(([id]) => id === tab) ? tab as Tab : earlier[tab];
};

/** Activity (D235; five views since Design A): what needs you, what agents are doing,
 * what they will do, everything they did, and how it is going. It opens on Needs you
 * unless a link asks for another view. */
export function ActivityPage({ href }: { href: string }) {
  const [params] = useState(() => new URL(href).searchParams);
  // A link to one run (from a schedule's posted result or its runs) opens it here.
  const [run, setRun] = useState(() => params.get('run') ?? undefined);
  // A link to one task (from a task chip in Chat) opens it in Work → Tasks.
  const [task] = useState(() => params.get('task') ?? undefined);
  const [tab, setTab] = useState<Tab>(() => chosen(href) ?? (run ? 'history' : task ? 'work' : 'needs'));
  const [show, setShow] = useState<'agents' | 'tasks'>(() => task || params.get('tab') === 'open' ? 'tasks' : 'agents');
  // The row of Needs you a link asked for (the way back from Chat, or an agent's panel).
  const [item, setItem] = useState(() => params.get('item') ?? undefined);
  const closeRun = () => {
    setRun(undefined);
    const url = new URL(window.location.href); url.searchParams.delete('run'); window.history.replaceState(null, '', url);
  };
  const [search, setSearch] = useState('');
  // Whose agents Work, Planned, History and Insights show; My agents by default (owner decision, 2026-11-01).
  const [scope, setScope] = useState<Scope>(savedScope);
  const pick = (next: Scope) => { setScope(next); saveScope(next); };
  const [since, setSince] = useState<number>();
  // History opened from Work starts on one type ("events not handled") or one agent.
  const [historyKind, setHistoryKind] = useState<string>();
  const [historyAgent, setHistoryAgent] = useState<string>();
  const attention = useAttention('needs_you', null);
  const count = attention.data?.items.length ?? 0;
  // A wide window reads its lists with the opened item beside them.
  const wide = useMediaQuery('(min-width: 1040px)');
  const split = wide && (tab === 'needs' || tab === 'history' || (tab === 'work' && show === 'agents'));
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const select = (next: Tab) => {
    setTab(next);
    if (next !== 'history') { setSince(undefined); setHistoryKind(undefined); setHistoryAgent(undefined); }
    if (next !== 'needs') setItem(undefined);
  };
  const history = (from?: number, kind?: string, agent?: string) => { setSince(from); setHistoryKind(kind); setHistoryAgent(agent); setTab('history'); };
  const keys = (event: React.KeyboardEvent, index: number) => {
    const moves: Record<string, number> = { ArrowLeft: index - 1, ArrowRight: index + 1, Home: 0, End: tabs.length - 1 };
    if (!(event.key in moves)) return;
    event.preventDefault();
    const next = (moves[event.key] + tabs.length) % tabs.length;
    select(tabs[next][0]); buttons.current[next]?.focus();
  };
  return <section className={`activity-page${split ? ' activity-split' : ''}`} aria-labelledby="activity-title">
    <header className="activity-header">
      <h1 id="activity-title">Activity</h1>
      <input type="search" className="activity-search" aria-label={tabs.find(([id]) => id === tab)![2]} placeholder={tabs.find(([id]) => id === tab)![2]}
        disabled={tab === 'insights'} value={search} onChange={event => setSearch(event.target.value)} />
    </header>
    <div className="activity-bar"><div className="activity-tabs" role="tablist" aria-label="Activity views">{tabs.map(([id, label], index) =>
      <button key={id} type="button" role="tab" id={`activity-tab-${id}`} aria-controls="activity-panel" aria-selected={tab === id}
        tabIndex={tab === id ? 0 : -1} ref={node => { buttons.current[index] = node; }}
        onClick={() => select(id)} onKeyDown={event => keys(event, index)}>
        {label}{id === 'needs' && count > 0 && <span className="activity-count" aria-label={`${count}${attention.data?.next_before ? ' or more' : ''} waiting`}>
          {count}{attention.data?.next_before ? '+' : ''}</span>}
      </button>)}</div>
      {tab === 'work' && <div className="activity-scope" role="group" aria-label="Show">
        {([['agents', 'Agents'], ['tasks', 'Tasks']] as const).map(([value, label]) =>
          <button key={value} type="button" aria-pressed={show === value} onClick={() => setShow(value)}>{label}</button>)}</div>}
      {tab !== 'needs' && <div className="activity-scope" role="group" aria-label="Whose agents">
        {([['mine', 'My agents'], ['all', 'All agents']] as const).map(([value, label]) =>
          <button key={value} type="button" aria-pressed={scope === value} onClick={() => pick(value)}>{label}</button>)}
      </div>}
      {/* Pausing every agent is rare and stops the whole workspace, so it is not a permanent button. */}
      {tab === 'work' && currentWorkspace()?.role === 'Admin' && <details className="activity-more">
        <summary aria-label="More">⋯</summary><div><PauseAgentsControl /></div></details>}</div>
    <div id="activity-panel" className="activity-panel" role="tabpanel" aria-labelledby={`activity-tab-${tab}`}>
      {tab === 'needs' && <NeedsMe key={item ?? ''} first={attention} search={search} pane={wide} initial={item} />}
      {tab === 'work' && show === 'agents' && <NowView scope={scope} search={search} waiting={attention.data?.items} pane={wide}
        openHistory={from => history(from)} openAgentHistory={agent => history(undefined, undefined, agent)} openUnhandled={() => history(undefined, 'unhandled')}
        openNeeds={key => { setItem(key); setTab('needs'); }} />}
      {tab === 'work' && show === 'tasks' && <OpenTasks scope={scope} search={search} initial={task} />}
      {tab === 'planned' && <UpcomingView scope={scope} search={search} />}
      {tab === 'insights' && <ReviewView scope={scope} />}
      {tab === 'history' && <HistoryView key={`${since ?? 'period'}.${historyKind ?? ''}.${historyAgent ?? ''}`} scope={scope} search={search} since={since}
        initialKind={historyKind} initialAgent={historyAgent} pane={wide} />}
    </div>
    {run && <RequestDetails reply={run} close={closeRun} />}
  </section>;
}
