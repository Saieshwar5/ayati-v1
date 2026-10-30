import { useCallback, useState } from 'react';
import { identityRequest, workspaceId, workspaceSession } from '../workspace-context';
import { Button, Dialog } from './ui';
import { useMutation, useResource } from './requests';
import type { Member } from './people';

// What the server reports a person is responsible for (member_handover.rs).
type Responsibilities = {
  agents: { id: string; name: string }[]; private_agents: number; library_items: number; kept_library_items: number;
  schedules: number; event_triggers: number; approvals: number; tool_rules: number; invitations: number;
};
const base = `/workspaces/${workspaceId}`;
export const personName = (member: Member) => member.display_name || member.email;
const count = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;
const lines = (...parts: (string | false)[]): string[] => parts.filter((part): part is string => Boolean(part));
const and = (...parts: (string | false)[]) => lines(...parts).join(' and ') || false;

/** Shows what removal moves, keeps, pauses and cancels, then removes in one request. */
export function RemovePersonDialog({ member, members, close, removed }: {
  member: Member; members: Member[]; close: () => void; removed: (message: string) => Promise<void>;
}) {
  const load = useCallback(() => identityRequest<Responsibilities>(`${base}/members/${member.id}/responsibilities`), [member.id]);
  const summary = useResource(load);
  const choices = members.filter(person => person.id !== member.id);
  const you = workspaceSession?.user?.id;
  const [heir, setHeir] = useState(choices.some(person => person.id === you) ? you! : choices[0]?.id ?? '');
  const mutation = useMutation();
  const name = personName(member);
  const heirName = (() => { const person = choices.find(choice => choice.id === heir); return person ? personName(person) : 'them'; })();
  const data = summary.data;
  const groups = data ? [
    { title: `Moves to ${heirName}`, tone: 'moves', rows: lines(
      data.agents.length > 0 && `${count(data.agents.length, 'shared agent', 'shared agents')}: ${data.agents.map(agent => agent.name).join(', ')}`,
      data.library_items > 0 && count(data.library_items, 'Library item shared with everyone', 'Library items shared with everyone')) },
    { title: 'Stays as it is', tone: 'stays', rows: lines(and(
      data.private_agents > 0 && count(data.private_agents, 'private agent', 'private agents'),
      data.kept_library_items > 0 && count(data.kept_library_items, 'private or selected Library item', 'private or selected Library items'))),
      note: `Never shared with ${heirName}, so they don’t move.` },
    { title: 'Paused for review', tone: 'paused', rows: lines(and(
      data.schedules > 0 && count(data.schedules, 'schedule', 'schedules'),
      data.event_triggers > 0 && count(data.event_triggers, 'event trigger', 'event triggers'))),
      note: `They run on ${name}’s authority, so the new agent owner reviews them and sets them up again.` },
    { title: 'Cancelled', tone: 'cancelled', rows: lines(
      data.approvals > 0 && `${count(data.approvals, 'approval', 'approvals')} waiting for ${name}; the agent will ask again`,
      data.tool_rules > 0 && `${count(data.tool_rules, 'tool rule', 'tool rules')} ${name} set up or approves, now set to Block`,
      data.invitations > 0 && `${count(data.invitations, 'invitation', 'invitations')} ${name} sent`) },
  ].filter(group => group.rows.length) : [];
  const remove = () => void mutation.run(async () => {
    await identityRequest(`${base}/members/${member.id}`, { role: null, handover_to: heir });
    await removed(`${name} was removed. The Audit log records what moved to ${heirName}.`);
  });
  return <Dialog title={`Remove ${name}?`} busy={mutation.busy} status={mutation.status || summary.error} close={close} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button className="danger settings-remove" disabled={mutation.busy || !data || !heir} onClick={remove}>{mutation.busy ? 'Removing…' : `Remove ${name}`}</Button>
  </>}>
    <p className="ui-help settings-remove-intro">{name} loses access to this workspace straight away: channels, agents and files. This is recorded in the Audit log.</p>
    <label className="ui-field">Who takes over<select value={heir} onChange={event => setHeir(event.target.value)}>
      {choices.map(person => <option key={person.id} value={person.id}>{personName(person)}{person.id === you ? ' (you)' : ''}</option>)}
    </select></label>
    {!data && !summary.error && <p role="status">Checking what {name} looks after…</p>}
    {summary.error && <Button onClick={() => void summary.refresh().catch(() => {})}>Retry</Button>}
    {data && !groups.length && <p className="ui-help">{name} doesn’t own or look after anything that needs handing over.</p>}
    {groups.map(group => <section key={group.title} className={`settings-handover ${group.tone}`} aria-label={group.title}>
      <h3>{group.title}</h3><ul>{group.rows.map(row => <li key={row}>{row}</li>)}</ul>{group.note && <p className="ui-help">{group.note}</p>}
    </section>)}
  </Dialog>;
}
