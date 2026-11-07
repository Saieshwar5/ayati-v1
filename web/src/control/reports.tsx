import { useResource } from '../react/requests';
import { control, when, type Report } from './api';
import type { Go } from './app';
import { Loading, Tag } from './shared';

const load = () => control<{ reports: Report[] }>('/reports');

/** What people sent from Settings → Help. They wrote these for the Cuckoo team. */
export function ReportsScreen({ go }: { go: Go }) {
  const { data, error } = useResource(load);
  if (!data) return <Loading error={error} label="problem reports" />;
  if (!data.reports.length) return <p className="cc-empty">No problem reports yet.</p>;
  return <ol className="cc-stack cc-narrow-page cc-plain">{data.reports.map(report => <li key={report.id} className="cc-card cc-pad">
    <div className="cc-between"><span><Tag tone="new">{report.topic.toUpperCase()}</Tag> <span className="cc-dim cc-small">{report.reporter} · {when(report.created_ms)}</span></span>
      <button type="button" className="cc-btn ghost" onClick={() => go('workspaces', { id: report.workspace })}>Open workspace</button></div>
    <p className="cc-report-body">{report.body}</p>
    {report.technical != null && <details className="cc-small"><summary>Browser and page facts</summary><pre className="cc-mono">{JSON.stringify(report.technical, null, 2)}</pre></details>}
  </li>)}</ol>;
}
