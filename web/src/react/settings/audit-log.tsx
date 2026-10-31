import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { chatRequest } from '../chat/model';

type Entry = { id: number; at: number; actor: string; action: string; kind: string; sentence: string };
type Page = { activity: Entry[]; next_before: number | null };
const filters: [string, string][] = [['all', 'All'], ['people', 'People'], ['connections', 'Connections'], ['approvals', 'Approvals'], ['admin_views', 'Admin views']];
const load = (kind: string, before?: number) => chatRequest<Page>(`/activity?kind=${kind}${before ? `&before=${before}` : ''}`);
function when(seconds: number): string {
  const at = new Date(seconds * 1000), today = new Date(); today.setHours(0, 0, 0, 0);
  const time = at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  if (at >= today) return `Today ${time}`;
  if (at.getTime() >= today.getTime() - 86400000) return `Yesterday ${time}`;
  return at.toLocaleDateString([], { day: 'numeric', month: 'short' }) + ` ${time}`;
}

/** Settings → Audit log (designs/settings 9): who changed access, connections
 * and approvals, as sentences, with a filter for each kind. Admins only. */
export function AuditLog() {
  const [kind, setKind] = useState('all');
  return <section className="settings-audit"><header className="settings-section-heading"><div><h2>Audit log</h2>
    <p>Who changed access, connections and approvals.</p></div><span className="ui-badge">Admins only</span></header>
    <div className="settings-audit-filters" role="group" aria-label="Show">{filters.map(([id, label]) =>
      <Button key={id} aria-pressed={kind === id} primary={kind === id} onClick={() => setKind(id)}>{label}</Button>)}</div>
    <Entries key={kind} kind={kind} />
  </section>;
}

function Entries({ kind }: { kind: string }) {
  const first = useResource(useCallback(() => load(kind), [kind]));
  const [older, setOlder] = useState<Page>();
  const more = useMutation();
  const entries = [...(first.data?.activity ?? []), ...(older?.activity ?? [])];
  const next = older ? older.next_before : first.data?.next_before;
  return <>
    {first.error && <p role="alert">{first.error}</p>}
    {!first.data && !first.error && <p role="status">Loading the audit log…</p>}
    {first.data && !entries.length && <p>Nothing recorded yet.</p>}
    <table className="settings-audit-table"><tbody>{entries.map(entry => <tr key={entry.id} className={entry.kind === 'admin_views' ? 'admin-view' : undefined}>
      <td>{when(entry.at)}</td><th scope="row">{entry.actor}</th><td>{entry.sentence}</td></tr>)}</tbody></table>
    {next && <Button disabled={more.busy} onClick={() => void more.run(async () => {
      const page = await load(kind, next);
      setOlder(old => ({ activity: [...(old?.activity ?? []), ...page.activity], next_before: page.next_before }));
    })}>{more.busy ? 'Loading…' : 'Show older'}</Button>}
    {more.status && <p role="alert">{more.status}</p>}
  </>;
}
