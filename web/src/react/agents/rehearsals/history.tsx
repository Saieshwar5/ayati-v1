import { Button } from '../../ui';
import { sampleLabel, type History } from './model';
export function SampleHistory({ data, loading, open, newer, older, refresh }: {
  data?: History; loading: boolean; open: (id: string) => void; newer?: () => void; older: (cursor: string) => void; refresh: () => void;
}) {
  return <section aria-label="Your saved samples"><p className="ui-help">Only your own samples with current job and source access appear here. Opening one never reruns it.</p>
    {!data && <p role="status">Loading saved samples…</p>}{data && !data.samples.length && <p>No saved samples available to you.</p>}
    {data?.samples.map(sample => <article className="sample-history-row" key={sample.id}><strong>{sample.task}</strong>
      <p>{sampleLabel(sample.state)} · Job version {sample.profile_revision} · {new Date(sample.created_ms).toLocaleString()}</p>
      <Button onClick={() => open(sample.id)}>Open sample</Button></article>)}
    <nav className="sample-actions" aria-label="Sample history pages">
      {newer && <Button disabled={loading} onClick={newer}>Newer samples</Button>}
      {data?.next_before && <Button disabled={loading} onClick={() => older(data.next_before!)}>Older samples</Button>}
      <Button disabled={loading} onClick={refresh}>Refresh saved samples</Button>
    </nav>
  </section>;
}
