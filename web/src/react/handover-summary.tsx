import { useCallback } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button } from './ui';
import { useResource } from './requests';

// What the server reports a person is responsible for (member_handover.rs).
type Responsibilities = {
  agents: { id: string; name: string }[]; private_agents: number; library_items: number; kept_library_items: number;
  schedules: number; event_triggers: number; invitations: number;
};
const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const lines = (...parts: (string | false)[]): string[] => parts.filter((part): part is string => Boolean(part));
const and = (...parts: (string | false)[]) => lines(...parts).join(' and ') || false;

/** What a person looks after; admins read anyone's, and people read their own before leaving. */
export function useResponsibilities(person: string) {
  const load = useCallback(() => identityRequest<Responsibilities>(`/workspaces/${workspaceId}/members/${person}/responsibilities`), [person]);
  return useResource(load);
}

/** What moves to the person taking over, what stays, what is paused and what is cancelled.
 * `self` words it for the person who is leaving. */
export function HandoverSummary({ summary, name, heirName, self = false }: {
  summary: ReturnType<typeof useResponsibilities>; name: string; heirName: string; self?: boolean;
}) {
  const data = summary.data;
  const who = self ? 'you' : name;
  const groups = data ? [
    { title: `Moves to ${heirName}`, tone: 'moves', rows: lines(
      data.agents.length > 0 && `${count(data.agents.length, 'agent', 'agents')}: ${data.agents.map(agent => agent.name).join(', ')}`,
      data.library_items > 0 && count(data.library_items, 'Library item shared with everyone', 'Library items shared with everyone')) },
    { title: 'Stays as it is', tone: 'stays', rows: lines(
      data.kept_library_items > 0 && count(data.kept_library_items, 'private or selected Library item', 'private or selected Library items')),
      note: `Never shared with ${heirName}, so they don’t move.` },
    { title: 'Paused for review', tone: 'paused', rows: lines(and(
      data.private_agents > 0 && count(data.private_agents, 'only-me agent', 'only-me agents'),
      data.schedules > 0 && count(data.schedules, 'schedule', 'schedules'),
      data.event_triggers > 0 && count(data.event_triggers, 'event trigger', 'event triggers'))),
      note: `They worked for ${self ? 'you' : name}, so ${heirName} reviews them before they run again.` },
    { title: 'Cancelled', tone: 'cancelled', rows: lines(
      data.invitations > 0 && `${count(data.invitations, 'invitation', 'invitations')} ${who} sent`) },
  ].filter(group => group.rows.length) : [];
  return <>
    {!data && !summary.error && <p role="status">Checking what {who} look{self ? '' : 's'} after…</p>}
    {summary.error && <Button onClick={() => void summary.refresh().catch(() => {})}>Retry</Button>}
    {data && !groups.length && <p className="ui-help">{self ? 'You don’t' : `${name} doesn’t`} own or look after anything that needs handing over.</p>}
    {groups.map(group => <section key={group.title} className={`settings-handover ${group.tone}`} aria-label={group.title}>
      <h3>{group.title}</h3><ul>{group.rows.map(row => <li key={row}>{row}</li>)}</ul>{group.note && <p className="ui-help">{group.note}</p>}
    </section>)}
  </>;
}
