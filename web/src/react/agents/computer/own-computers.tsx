import { useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation } from '../../requests';
import { identityRequest, workspaceId } from '../../../workspace-context';
import { base, type Agent } from '../model';
import './computer.css';

/** One machine the workspace runs itself (schema 119). Keys and codes are never listed. */
export type OwnComputer = {
  id: string; name: string; state: 'waiting' | 'online' | 'offline'; platform: string | null;
  revision: number; busy: boolean; agents: { id: string; name: string }[];
};
export type OwnComputersView = { computers?: OwnComputer[]; can_change: boolean };
type Code = { id: string; code: string; expires_minutes: number };

const path = `/workspaces/${workspaceId}/workspace-computers`;
export const loadOwnComputers = () => identityRequest<OwnComputersView>(path);
const states = { waiting: 'Waiting for its connector', online: 'Online', offline: 'Offline' };

/** The workspace's own computer this agent works on, if an admin chose one. */
export const ownComputerOf = (view: OwnComputersView | undefined, agent: string) =>
  view?.computers?.find(computer => computer.agents.some(row => row.id === agent));

/** The command an admin runs on the machine; the code works once, for 15 minutes. */
export function connectorCommand(id: string, code: string) {
  return `python3 cuckoo_connector.py --server ${window.location.origin} --computer ${id} --code ${code}`;
}

/** Agent → Connections → Your own computer (B3): an admin adds a machine the
 * workspace runs itself, such as an office PC with Tally, and chooses here whether
 * this agent works there instead of on Cuckoo's computer. Members only see it. */
export function OwnComputerCard({ agent, view, cuckoo, refresh }: {
  agent: Agent; view: OwnComputersView; /** This Cuckoo offers its own computers. */ cuckoo: boolean; refresh: () => Promise<void>;
}) {
  const mutation = useMutation();
  const [name, setName] = useState('');
  const [code, setCode] = useState<Code & { name: string }>();
  const [removing, setRemoving] = useState<OwnComputer>();
  // One id per typed name, so a repeated click adds one computer, not two.
  const attempt = useRef<{ name: string; id: string }>(undefined);
  const computers = view.computers ?? [];
  const editable = view.can_change;
  const chosen = ownComputerOf(view, agent.id);
  const reload = () => refresh().catch(() => {});
  const choose = (computer?: OwnComputer) => void mutation.run(async () => {
    await identityRequest(`${base}/${encodeURIComponent(agent.id)}/computer/own`, { computer: computer?.id ?? null });
    await reload();
    mutation.setStatus(computer ? `${agent.name} works on ${computer.name} from its next task.`
      : cuckoo ? `${agent.name} uses Cuckoo’s computer again, if it is on.` : `${agent.name} no longer works on a computer of yours.`);
  });
  const add = () => void mutation.run(async () => {
    const trimmed = name.trim();
    if (attempt.current?.name !== trimmed) attempt.current = { name: trimmed, id: crypto.randomUUID() };
    const added = await identityRequest<Code & { repeat?: boolean }>(path, { id: attempt.current.id, name: trimmed });
    if (added.code) setCode({ ...added, name: trimmed });
    attempt.current = undefined;
    setName('');
    await reload();
  });
  const newCode = (computer: OwnComputer) => void mutation.run(async () => {
    const fresh = await identityRequest<Code>(`${path}/${computer.id}/code`, { revision: computer.revision });
    setCode({ ...fresh, name: computer.name });
    await reload();
  });
  const remove = (computer: OwnComputer) => void mutation.run(async () => {
    await identityRequest(`${path}/${computer.id}/remove`, { revision: computer.revision });
    setRemoving(undefined);
    mutation.setStatus(`${computer.name} was removed. Its agents use Cuckoo’s computers again, if theirs are on.`);
    await reload();
  });
  return <section className="agent-computer agent-own-computer" aria-labelledby="agent-own-computer-title">
    <header className="agent-computer-head">
      <span><strong id="agent-own-computer-title">Your own computer</strong>
        <small>Let {agent.name} work on a machine you run, such as the office PC with your accounts software</small></span>
    </header>
    <p className="ui-help">The Cuckoo Connector on the machine connects out to Cuckoo, so no ports are opened. {agent.name} runs commands and
      handles files there; the browser isn’t available there yet.</p>
    <p className="ui-help" role="note">Agents run commands there with the permissions of the account that runs the connector. Use an account
      that can reach only what agents need, and keep backups.</p>
    {!computers.length && <p className="ui-help">No computers added yet.</p>}
    {computers.length > 0 && <fieldset className="agent-own-computers" disabled={mutation.busy}>
      <legend>Where {agent.name} works</legend>
      <ul>
        <li><label className="ui-check"><input type="radio" name={`own-computer-${agent.id}`} checked={!chosen} disabled={!editable} onChange={() => choose()} />
          <span><strong>{cuckoo ? 'Cuckoo’s computer' : 'No computer of yours'}</strong>
            {cuckoo && <small>The computer above, when it is on</small>}</span></label></li>
        {computers.map(computer => <li key={computer.id}>
          <label className="ui-check"><input type="radio" name={`own-computer-${agent.id}`} checked={chosen?.id === computer.id} disabled={!editable}
            onChange={() => choose(computer)} />
            <span><strong>{computer.name}</strong>
              <small>{states[computer.state]}{computer.platform ? ` · ${computer.platform}` : ''}{computer.busy ? ' · in use' : ''}</small>
              <small>{computer.agents.length ? `Used by ${computer.agents.map(row => row.name).join(', ')}` : 'No agents use it yet'}</small></span></label>
          {editable && <span className="agent-own-computer-actions">
            <Button aria-label={`New code for ${computer.name}`} onClick={() => newCode(computer)}>New code</Button>
            <Button aria-label={`Remove ${computer.name}`} onClick={() => setRemoving(computer)}>Remove</Button>
          </span>}
        </li>)}
      </ul>
    </fieldset>}
    {editable && <form className="agent-own-computer-add" onSubmit={event => { event.preventDefault(); add(); }}>
      <label className="ui-field">Computer name<input value={name} maxLength={100} placeholder="Office PC" onChange={event => setName(event.target.value)} /></label>
      <Button primary type="submit" disabled={mutation.busy || !name.trim()}>Add computer</Button>
    </form>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {!editable && <p className="ui-help">Only admins add computers and choose where {agent.name} works.</p>}
    {code && <Dialog title={`Connect ${code.name}`} close={() => setCode(undefined)} footer={<Button primary onClick={() => setCode(undefined)}>Done</Button>}>
      <p>On {code.name}, install Python 3, download the Cuckoo Connector and run this. The code works once, for {code.expires_minutes} minutes,
        and isn’t shown again.</p>
      <code className="agent-own-computer-code">{connectorCommand(code.id, code.code)}</code>
      <p className="ui-help">Agents work in their own folder under the connector’s folder. If the machine is replaced, get a new code: the old one stops at once.</p>
    </Dialog>}
    {removing && <Dialog title={`Remove ${removing.name}?`} close={() => setRemoving(undefined)} busy={mutation.busy} status={mutation.status}
      footer={<Button primary disabled={mutation.busy} onClick={() => remove(removing)}>Remove computer</Button>}>
      <p>Agents stop using it at once and its connector is signed out. Files on the machine stay there.</p>
    </Dialog>}
  </section>;
}
