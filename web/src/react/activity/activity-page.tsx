import { useRef, useState } from 'react';
import { useAttention } from '../attention/use-attention';
import { NeedsMe } from './needs-me';
import { NowView } from './now';
import { HistoryView } from './history';
import { RequestDetails } from './request-details';
import { saveScope, savedScope, type Scope } from './model';
import './activity.css';

type Tab = 'needs' | 'now' | 'history';
const tabs: [Tab, string][] = [['needs', 'Needs me'], ['now', 'Now'], ['history', 'History']];
const chosen = (href: string): Tab | undefined => {
  const tab = new URL(href).searchParams.get('tab');
  return tab === 'needs' || tab === 'now' || tab === 'history' ? tab : undefined;
};

/** Activity (D235): what needs me, what agents are doing now, and everything
 * they did. It opens on Needs me unless a link asks for another view. */
export function ActivityPage({ href }: { href: string }) {
  // A link to one run (from a schedule's posted result or its runs) opens it here.
  const [run, setRun] = useState(() => new URL(href).searchParams.get('run') ?? undefined);
  const [tab, setTab] = useState<Tab>(() => chosen(href) ?? (run ? 'history' : 'needs'));
  const closeRun = () => {
    setRun(undefined);
    const url = new URL(window.location.href); url.searchParams.delete('run'); window.history.replaceState(null, '', url);
  };
  const [search, setSearch] = useState('');
  // Whose agents Now and History show; My agents by default (owner decision, 2026-11-01).
  const [scope, setScope] = useState<Scope>(savedScope);
  const pick = (next: Scope) => { setScope(next); saveScope(next); };
  const [since, setSince] = useState<number>();
  const attention = useAttention('needs_you', null);
  const count = attention.data?.items.length ?? 0;
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const select = (next: Tab) => { setTab(next); if (next !== 'history') setSince(undefined); };
  const keys = (event: React.KeyboardEvent, index: number) => {
    const moves: Record<string, number> = { ArrowLeft: index - 1, ArrowRight: index + 1, Home: 0, End: tabs.length - 1 };
    if (!(event.key in moves)) return;
    event.preventDefault();
    const next = (moves[event.key] + tabs.length) % tabs.length;
    select(tabs[next][0]); buttons.current[next]?.focus();
  };
  return <section className="activity-page" aria-labelledby="activity-title">
    <header className="activity-header">
      <h1 id="activity-title">Activity</h1>
      <input type="search" className="activity-search" aria-label="Search activity" placeholder="Search activity"
        value={search} onChange={event => setSearch(event.target.value)} />
    </header>
    <div className="activity-tabs" role="tablist" aria-label="Activity views">{tabs.map(([id, label], index) =>
      <button key={id} type="button" role="tab" id={`activity-tab-${id}`} aria-controls="activity-panel" aria-selected={tab === id}
        tabIndex={tab === id ? 0 : -1} ref={node => { buttons.current[index] = node; }}
        onClick={() => select(id)} onKeyDown={event => keys(event, index)}>
        {label}{id === 'needs' && count > 0 && <span className="activity-count" aria-label={`${count}${attention.data?.next_before ? ' or more' : ''} waiting`}>
          {count}{attention.data?.next_before ? '+' : ''}</span>}
      </button>)}
      {tab !== 'needs' && <div className="activity-scope" role="group" aria-label="Whose agents">
        {([['mine', 'My agents'], ['all', 'All agents']] as const).map(([value, label]) =>
          <button key={value} type="button" aria-pressed={scope === value} onClick={() => pick(value)}>{label}</button>)}
      </div>}</div>
    <div id="activity-panel" className="activity-panel" role="tabpanel" aria-labelledby={`activity-tab-${tab}`}>
      {tab === 'needs' && <NeedsMe first={attention} search={search} />}
      {tab === 'now' && <NowView scope={scope} search={search} openHistory={from => { setSince(from); setTab('history'); }} />}
      {tab === 'history' && <HistoryView key={since ?? 'period'} scope={scope} search={search} since={since} />}
    </div>
    {run && <RequestDetails reply={run} close={closeRun} />}
  </section>;
}
