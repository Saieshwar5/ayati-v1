import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceSession } from '../../workspace-context';
import { changeOwners, type Agent, type Owner } from './model';
import { loadMembers } from './create/people-step';

/** The most owners one agent has (agent_owners.rs). */
const MOST = 10;

/** Adds and removes an agent's owners. Owners are equal: any owner, and every
 * admin, adds or removes the others without asking them. The last owner can't
 * be removed. An agent whose last owner left gets one here, and starts again
 * if it was working (schema 122). */
export function OwnersDialog({ agent, close, saved }: {
  agent: Agent; close: () => void; saved: (message: string) => Promise<void>;
}) {
  const me = workspaceSession?.user?.id ?? '';
  const members = useResource(loadMembers);
  const [owners, setOwners] = useState<Owner[]>(agent.owners ?? []);
  const [revision, setRevision] = useState(agent.revision);
  const [adding, setAdding] = useState('');
  // The last change made here, reported once the dialog closes.
  const [changed, setChanged] = useState('');
  const mutation = useMutation();
  const finish = () => { if (changed) void saved(changed); else close(); };
  const others = (members.data?.users ?? []).filter(member => !owners.some(owner => owner.id === member.id));
  const change = (step: { add: string } | { remove: string }, message: string) => void mutation.run(async () => {
    const next = await changeOwners(agent.id, step, revision);
    setOwners(next.owners);
    setRevision(next.revision);
    setAdding('');
    // Without ownership a member may no longer change it, so the page reloads.
    if ('remove' in step && step.remove === me) { await saved(message); return; }
    setChanged(message);
    mutation.setStatus(message);
  });
  const add = () => {
    const person = others.find(member => member.id === adding);
    if (person) change({ add: person.id }, `${person.name} is now an owner of ${agent.name}.`);
  };
  const last = owners.length === 1;
  return <Dialog title={owners.length ? `${agent.name} · Owners` : `Give ${agent.name} an owner`} close={finish}
    busy={mutation.busy} status={mutation.status} footer={<Button primary disabled={mutation.busy} onClick={finish}>Done</Button>}>
    {owners.length > 0 && <ul className="agent-owner-list" aria-label={`Owners of ${agent.name}`}>
      {owners.map(owner => <li key={owner.id}>
        <strong>{owner.name}{owner.id === me && ' (you)'}</strong>
        <Button disabled={mutation.busy || last}
          onClick={() => change({ remove: owner.id }, owner.id === me ? `You stopped being an owner of ${agent.name}.` : `${owner.name} is no longer an owner of ${agent.name}.`)}>
          {owner.id === me ? 'Stop being an owner' : `Remove ${owner.name}`}</Button>
      </li>)}
    </ul>}
    {!owners.length && <p className="ui-help">{agent.name}’s last owner left the workspace, so it is paused. Once it has an owner, it starts again if it was working.</p>}
    {last && <p className="ui-help">{agent.name} needs at least one owner. Add another before removing {owners[0].name}.</p>}
    {!members.data && !members.error && <p role="status">Loading people…</p>}
    {members.error && <p role="alert">{members.error}</p>}
    {owners.length < MOST
      ? <div className="agent-owner-add">
        <label className="ui-field">Add an owner<select value={adding} disabled={mutation.busy} onChange={event => setAdding(event.target.value)}>
          <option value="">Choose a person</option>
          {others.map(member => <option key={member.id} value={member.id}>{member.name}</option>)}
        </select></label>
        <Button disabled={mutation.busy || !adding} onClick={add}>Add owner</Button>
      </div>
      : <p className="ui-help">An agent can have up to {MOST} owners.</p>}
    <p className="ui-help">Owners are equal. Every owner can change {agent.name}, approve its actions and see all its work,
      including requests from private chats with it. Each one can pause it and add or remove owners without asking the others.
      Admins can do the same for every agent.{agent.audience === 'only_me' && ` ${agent.name} is set to Only me, so only its owners can talk to it.`}</p>
  </Dialog>;
}
