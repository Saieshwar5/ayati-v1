import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { chatRequest } from '../chat/model';
import { PersonFace } from '../faces';

type Entry = { id: number; at: number; actor: string; action: string; kind: string; sentence: string };
type Page = { activity: Entry[]; next_before: number | null };
const filters: [string, string][] = [['all', 'All'], ['people', 'People'], ['connections', 'Apps'], ['approvals', 'Approvals'], ['admin_views', 'Admin views']];
const load = (kind: string, before?: number) => chatRequest<Page>(`/activity?kind=${kind}${before ? `&before=${before}` : ''}`);
const startOfDay = (date: Date) => new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
/** "Today", "Yesterday" or the date, for the heading over a day's entries. */
export function dayHeading(seconds: number, now = new Date()): string {
  const day = startOfDay(new Date(seconds * 1000)), today = startOfDay(now);
  if (day === today) return 'Today';
  if (day === today - 86400000) return 'Yesterday';
  return new Date(day).toLocaleDateString([], { day: 'numeric', month: 'long', ...(new Date(day).getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }) });
}
const timeOf = (seconds: number) => new Date(seconds * 1000).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' });

/** Settings → Audit log: who changed access, apps and approvals, as sentences
 * grouped by day, with a filter for each kind. Admins only. */
export function AuditLog() {
  const [kind, setKind] = useState('all');
  return <section className="settings-audit"><h2>Audit log</h2>
    <p>Who changed people, apps, rules and approvals.</p>
    <div className="settings-filters" role="group" aria-label="Show">{filters.map(([id, label]) =>
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
  // Entries arrive newest first; a new heading starts wherever the day changes.
  const days: { heading: string; entries: Entry[] }[] = [];
  for (const entry of entries) {
    const heading = dayHeading(entry.at);
    if (days.at(-1)?.heading === heading) days.at(-1)!.entries.push(entry); else days.push({ heading, entries: [entry] });
  }
  return <>
    {first.error && <p role="alert">{first.error}</p>}
    {!first.data && !first.error && <p role="status">Loading the audit log…</p>}
    {first.data && !entries.length && <p className="ui-empty">Nothing recorded here yet.</p>}
    {days.map(day => <section key={day.heading} className="settings-audit-day" aria-label={day.heading}><h3>{day.heading}</h3>
      <ul>{day.entries.map(entry => <li key={entry.id} className={entry.kind === 'admin_views' ? 'admin-view' : undefined}>
        <time dateTime={new Date(entry.at * 1000).toISOString()}>{timeOf(entry.at)}</time>
        <PersonFace person={{ id: entry.actor, name: entry.actor }} className="settings-audit-face" />
        <p><strong>{entry.actor}</strong> {entry.sentence}</p>
        {entry.kind === 'admin_views' && <span className="ui-badge settings-waiting">Admin view</span>}
      </li>)}</ul></section>)}
    {next && <Button disabled={more.busy} onClick={() => void more.run(async () => {
      const page = await load(kind, next);
      setOlder(old => ({ activity: [...(old?.activity ?? []), ...page.activity], next_before: page.next_before }));
    })}>{more.busy ? 'Loading…' : 'Show older'}</Button>}
    {more.status && <p role="alert">{more.status}</p>}
  </>;
}
