import { useCallback, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { identityRequest, workspaceId } from '../../workspace-context';

/** One machine the workspace runs itself (schema 119). Keys and codes are never listed. */
export type WorkspaceComputer = {
  id: string; name: string; state: 'waiting' | 'online' | 'offline'; platform: string | null;
  revision: number; busy: boolean; agents: { id: string; name: string }[];
};
type View = { computers?: WorkspaceComputer[]; can_change: boolean };
type Code = { id: string; code: string; expires_minutes: number };
type AgentRow = { id: string; name: string };

const path = `/workspaces/${workspaceId}/workspace-computers`;
const load = () => identityRequest<View>(path);
const loadAgents = () => identityRequest<{ agents: AgentRow[] }>(`/workspaces/${workspaceId}/agent-profiles`);
const states = { waiting: 'Waiting for its connector', online: 'Online', offline: 'Offline' };

/** The command an admin runs on the machine; the code works once, for 15 minutes. */
export function connectorCommand(id: string, code: string) {
  return `python3 cuckoo_connector.py --server ${window.location.origin} --computer ${id} --code ${code}`;
}

/** Settings → Computers → Your computers (B3): machines the workspace runs
 * itself, such as an office PC with Tally. Agents chosen here run their
 * commands and files there instead of on Cuckoo's computers. */
export function WorkspaceComputers() {
  const view = useResource(load);
  const mutation = useMutation();
  const [name, setName] = useState('');
  const [code, setCode] = useState<Code & { name: string }>();
  const [choosing, setChoosing] = useState<WorkspaceComputer>();
  const [removing, setRemoving] = useState<WorkspaceComputer>();
  // One id per typed name, so a repeated click adds one computer, not two.
  const attempt = useRef<{ name: string; id: string }>(undefined);
  const computers = view.data?.computers ?? [];
  const editable = Boolean(view.data?.can_change);
  const add = () => void mutation.run(async () => {
    const trimmed = name.trim();
    if (attempt.current?.name !== trimmed) attempt.current = { name: trimmed, id: crypto.randomUUID() };
    const added = await identityRequest<Code & { repeat?: boolean }>(path, { id: attempt.current.id, name: trimmed });
    if (added.code) setCode({ ...added, name: trimmed });
    attempt.current = undefined;
    setName('');
    await view.refresh().catch(() => {});
  });
  const newCode = (computer: WorkspaceComputer) => void mutation.run(async () => {
    const fresh = await identityRequest<Code>(`${path}/${computer.id}/code`, { revision: computer.revision });
    setCode({ ...fresh, name: computer.name });
    await view.refresh().catch(() => {});
  });
  const remove = (computer: WorkspaceComputer) => void mutation.run(async () => {
    await identityRequest(`${path}/${computer.id}/remove`, { revision: computer.revision });
    setRemoving(undefined);
    mutation.setStatus(`${computer.name} was removed. Its agents use Cuckoo’s computers again, if theirs are on.`);
    await view.refresh().catch(() => {});
  });
  return <section className="settings-workspace-computers" aria-labelledby="workspace-computers-title">
    <h3 id="workspace-computers-title">Your computers</h3>
    <p className="ui-help">Let chosen agents work on a machine you run, such as the office PC with your accounts software. The Cuckoo Connector on it
      connects out to Cuckoo, so no ports are opened.</p>
    <p className="ui-help" role="note">Agents run commands there with the permissions of the account that runs the connector. Use an account
      that can reach only what agents need, and keep backups.</p>
    {view.error && <p role="alert">{view.error}</p>}
    {!view.data && !view.error && <p role="status">Loading your computers…</p>}
    {view.data && !computers.length && <p className="ui-help">No computers added yet.</p>}
    {computers.length > 0 && <ul className="settings-computer-list">
      {computers.map(computer => <li key={computer.id}>
        <span><strong>{computer.name}</strong>
          <small>{states[computer.state]}{computer.platform ? ` · ${computer.platform}` : ''}{computer.busy ? ' · in use' : ''}</small>
          <small>{computer.agents.length ? `Used by ${computer.agents.map(agent => agent.name).join(', ')}` : 'No agents use it yet'}</small></span>
        {editable && <span className="settings-computer-actions">
          <Button disabled={mutation.busy} onClick={() => setChoosing(computer)}>Choose agents</Button>
          <Button disabled={mutation.busy} onClick={() => newCode(computer)}>New code</Button>
          <Button disabled={mutation.busy} onClick={() => setRemoving(computer)}>Remove</Button>
        </span>}
      </li>)}
    </ul>}
    {editable && <form className="settings-computer-add" onSubmit={event => { event.preventDefault(); add(); }}>
      <label className="ui-field">Computer name<input value={name} maxLength={100} placeholder="Office PC" onChange={event => setName(event.target.value)} /></label>
      <Button primary type="submit" disabled={mutation.busy || !name.trim()}>Add computer</Button>
    </form>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {view.data && !editable && <p className="ui-help">Only admins add and change these.</p>}
    {code && <Dialog title={`Connect ${code.name}`} close={() => setCode(undefined)} footer={<Button primary onClick={() => setCode(undefined)}>Done</Button>}>
      <p>On {code.name}, install Python 3, download the Cuckoo Connector and run this. The code works once, for {code.expires_minutes} minutes,
        and isn’t shown again.</p>
      <code className="settings-computer-code">{connectorCommand(code.id, code.code)}</code>
      <p className="ui-help">Agents work in their own folder under the connector’s folder. If the machine is replaced, get a new code: the old one stops at once.</p>
    </Dialog>}
    {choosing && <ChooseAgents computer={choosing} close={() => setChoosing(undefined)} saved={async () => {
      setChoosing(undefined);
      await view.refresh().catch(() => {});
    }} />}
    {removing && <Dialog title={`Remove ${removing.name}?`} close={() => setRemoving(undefined)} busy={mutation.busy} status={mutation.status}
      footer={<Button primary disabled={mutation.busy} onClick={() => remove(removing)}>Remove computer</Button>}>
      <p>Agents stop using it at once and its connector is signed out. Files on the machine stay there.</p>
    </Dialog>}
  </section>;
}

/** Each agent uses one computer, so choosing one here moves it from another machine. */
function ChooseAgents({ computer, close, saved }: { computer: WorkspaceComputer; close: () => void; saved: () => Promise<void> }) {
  const agents = useResource(useCallback(loadAgents, []));
  const mutation = useMutation();
  const [chosen, setChosen] = useState(() => computer.agents.map(agent => agent.id));
  const save = () => void mutation.run(async () => {
    await identityRequest(`${path}/${computer.id}/agents`, { revision: computer.revision, agents: chosen });
    await saved();
  });
  return <Dialog title={`Agents that use ${computer.name}`} close={close} busy={mutation.busy} status={mutation.status}
    footer={<Button primary disabled={mutation.busy || !agents.data} onClick={save}>Save agents</Button>}>
    {agents.error && <p role="alert">{agents.error}</p>}
    {!agents.data && !agents.error && <p role="status">Loading agents…</p>}
    {agents.data && <fieldset className="settings-computer-agents"><legend>Agents</legend>
      {agents.data.agents.map(agent => <label key={agent.id} className="ui-check"><input type="checkbox" checked={chosen.includes(agent.id)}
        onChange={event => setChosen(event.target.checked ? [...chosen, agent.id] : chosen.filter(id => id !== agent.id))} />{agent.name}</label>)}
    </fieldset>}
    <p className="ui-help">An agent chosen here stops using another computer of yours or Cuckoo’s. Its Programs rule still decides which commands need approval.</p>
  </Dialog>;
}
