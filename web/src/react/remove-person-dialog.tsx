import { useState } from 'react';
import { identityRequest, workspaceId, workspaceSession } from '../workspace-context';
import { Button, Dialog } from './ui';
import { useMutation } from './requests';
import { HandoverSummary, ownedAlone, useResponsibilities } from './handover-summary';
import type { Member } from './people';

export const personName = (member: Member) => member.display_name || member.email;

/** Shows what removal moves, keeps, pauses and cancels, then removes in one request. */
export function RemovePersonDialog({ member, members, close, removed }: {
  member: Member; members: Member[]; close: () => void; removed: (message: string) => Promise<void>;
}) {
  const summary = useResponsibilities(member.id);
  const choices = members.filter(person => person.id !== member.id);
  const you = workspaceSession?.user?.id;
  const [heir, setHeir] = useState(choices.some(person => person.id === you) ? you! : choices[0]?.id ?? '');
  // Agents they own alone pause with no owner unless the admin names one here;
  // agents with other owners stay with them (schema 122).
  const [owner, setOwner] = useState('');
  const mutation = useMutation();
  const name = personName(member);
  const named = (id: string) => { const person = choices.find(choice => choice.id === id); return person ? personName(person) : undefined; };
  const heirName = named(heir) ?? 'them';
  const ownsAgents = ownedAlone(summary.data).length > 0;
  const remove = () => void mutation.run(async () => {
    await identityRequest(`/workspaces/${workspaceId}/members/${member.id}`, { role: null, handover_to: heir, ...(ownsAgents && owner ? { agents_owner: owner } : {}) });
    await removed(ownsAgents && !owner
      ? `${name} was removed. Their agents are paused until you give each one a new owner.`
      : `${name} was removed. The Audit log records what moved.`);
  });
  return <Dialog title={`Remove ${name}?`} busy={mutation.busy} status={mutation.status || summary.error} close={close} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button className="danger settings-remove" disabled={mutation.busy || !summary.data || !heir} onClick={remove}>{mutation.busy ? 'Removing…' : `Remove ${name}`}</Button>
  </>}>
    <p className="ui-help settings-remove-intro">{name} loses access to this workspace straight away: channels, agents and files. This is recorded in the Audit log.</p>
    <div className="ui-form">
    {ownsAgents && <label className="ui-field">New owner of agents only they own<select value={owner} onChange={event => setOwner(event.target.value)}>
      <option value="">Nobody yet: pause their agents</option>
      {choices.map(person => <option key={person.id} value={person.id}>{personName(person)}{person.id === you ? ' (you)' : ''}</option>)}
    </select></label>}
    <label className="ui-field">Who takes over their Library items<select value={heir} onChange={event => setHeir(event.target.value)}>
      {choices.map(person => <option key={person.id} value={person.id}>{personName(person)}{person.id === you ? ' (you)' : ''}</option>)}
    </select></label>
    </div>
    <HandoverSummary summary={summary} name={name} heirName={heirName} agentsOwner={ownsAgents ? named(owner) : undefined} />
  </Dialog>;
}
