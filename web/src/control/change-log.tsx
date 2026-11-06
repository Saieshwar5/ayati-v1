import { useCallback, useState } from 'react';
import { useMutation, useResource } from '../react/requests';
import { control, when, type LogEntry } from './api';
import { Loading, Tag } from './shared';

type Page = { entries: LogEntry[]; more: boolean };
const AREAS = ['all', 'workspaces', 'providers', 'models', 'routing', 'settings', 'codes'] as const;

export function ChangeLogScreen() {
  const [area, setArea] = useState<(typeof AREAS)[number]>('all');
  const load = useCallback(() => control<Page>(`/log?area=${area}`), [area]);
  const { data, error, setData } = useResource(load);
  const older = useMutation();
  return <div className="cc-stack cc-narrow-page">
    <div className="cc-toolbar" role="group" aria-label="Area">
      {AREAS.map(a => <button key={a} type="button" className="cc-chip" aria-pressed={area === a} onClick={() => setArea(a)}>{a === 'all' ? 'All' : a[0].toUpperCase() + a.slice(1)}</button>)}
    </div>
    <section className="cc-card" aria-label="Change log">
      {!data ? <Loading error={error} label="the change log" /> : !data.entries.length ? <p className="cc-empty">No changes yet.</p> :
        <ol className="cc-log">{data.entries.map(entry => <li key={entry.id}>
          <span className="cc-dim cc-num cc-small">{when(entry.at_ms)}</span>
          <div><div><strong>{entry.by}</strong> {entry.summary}</div><div className="cc-dim cc-small">“{entry.reason}”</div></div>
          <span className="cc-log-tags"><Tag tone="off">{entry.area?.toUpperCase()}</Tag> <Tag tone={entry.via === 'cli' ? 'new' : 'off'}>{entry.via.toUpperCase()}</Tag></span>
        </li>)}</ol>}
      {data?.more && <div className="cc-pad"><button type="button" className="cc-btn" disabled={older.busy} onClick={() => void older.run(async () => {
        const last = data.entries.at(-1)?.id;
        const next = await control<Page>(`/log?area=${area}&before=${last}`);
        setData({ entries: [...data.entries, ...next.entries], more: next.more });
      })}>{older.busy ? 'Loading…' : 'Show older'}</button>{older.status && <p className="cc-alert" role="alert">{older.status}</p>}</div>}
    </section>
  </div>;
}
