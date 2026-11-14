import { useCallback, useState } from 'react';
import './systems.css';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { modes } from '../control-model';
import type { Agent } from '../model';
import { ConnectSystem } from './connect-system';
import { changeSystem, initialChoices, loadSystems, reviewSystem, savedTools, stateLabels, type ListedTool, type System } from './model';
import { ReviewTools, chosen } from './review-tools';

/** "Your systems" on the agent's Tools tab (C1, D249): a tile per connected
 * system with its state, and Connect a system for the owner and admins. */
export function SystemsSection({ agent }: { agent: Agent }) {
  const load = useCallback(() => loadSystems(agent.id), [agent.id]);
  const { data, refresh } = useResource(load);
  const [connecting, setConnecting] = useState(false);
  const [reviewing, setReviewing] = useState<System>();
  const [removing, setRemoving] = useState<System>();
  const m = useMutation();
  const change = (system: System, to: 'on' | 'off' | 'remove', done: string) => void m.run(async () => {
    await changeSystem(agent.id, system.id, system.revision, to); await refresh(); setRemoving(undefined); m.setStatus(done);
  });
  const systems = Array.isArray(data?.systems) ? data.systems : [];
  const canConnect = data?.can_connect === true;
  // Nothing to show to people who can't connect one, or while it can't load.
  if (!canConnect && !systems.length) return null;
  return <section className="agent-systems" aria-label="Your systems">
    <h3 className="agent-group-label">Your systems</h3>
    <p className="ui-status" role="status">{m.busy ? 'Saving…' : m.status}</p>
    {systems.map(system => <article key={system.id} className="agent-tool system-tile" aria-label={system.name}>
      <header>
        <span><span className="agent-app-name"><strong>{system.name}</strong>
          <span className={`system-state ${system.state}`}>{stateLabels[system.state]}</span></span>
          <small>{system.tools.length} {system.tools.length === 1 ? 'tool' : 'tools'} on · {system.url}{system.last_call_ms ? ` · last call ${new Date(system.last_call_ms).toLocaleString()}` : ' · no calls yet'}</small>
          {system.state === 'changed' && <small className="agent-tool-health">{system.held.length} {system.held.length === 1 ? 'tool changed' : 'tools changed'} since the last review and {system.held.length === 1 ? 'is' : 'are'} held. The others work.</small>}
          {system.state === 'unreachable' && <small className="agent-tool-health">Cuckoo couldn’t reach it at the last check. Check the address or the key.</small>}
        </span>
        {canConnect && <span className="system-actions">
          <Button disabled={m.busy} onClick={() => setReviewing(system)}>{system.state === 'changed' ? `Review ${system.held.length} ${system.held.length === 1 ? 'tool' : 'tools'}` : 'Review tools'}</Button>
          <Button disabled={m.busy} onClick={() => change(system, system.state === 'off' ? 'on' : 'off', system.state === 'off' ? `${system.name} is on.` : `${system.name} is off. Nothing is sent to it.`)}>{system.state === 'off' ? 'Switch on' : 'Switch off'}</Button>
          <Button disabled={m.busy} onClick={() => setRemoving(system)}>Remove</Button>
        </span>}
      </header>
      <ul className="system-tile-tools">{system.tools.map(tool => <li key={tool.name}><span>{tool.name}</span>
        <small>{tool.kind === 'read' ? 'Reads' : 'Changes'} · {modes[tool.rule]}{system.held.includes(tool.name) ? ' · held' : ''}</small></li>)}</ul>
    </article>)}
    {canConnect && <Button onClick={() => setConnecting(true)}>Connect a system</Button>}
    {connecting && <ConnectSystem agent={agent.id} agentName={agent.name} close={() => setConnecting(false)} saved={() => refresh()} />}
    {reviewing && <ReviewSystem agent={agent.id} system={reviewing} close={() => setReviewing(undefined)} saved={() => refresh()} />}
    {removing && <Dialog title={`Remove ${removing.name}?`} close={() => setRemoving(undefined)} busy={m.busy} status={m.status}
      footer={<><Button onClick={() => setRemoving(undefined)}>Keep it</Button><Button primary onClick={() => change(removing, 'remove', `${removing.name} was removed.`)}>Remove</Button></>}>
      <p>{agent.name} stops using its tools at once. Waiting approvals are cancelled. Activity keeps the record of earlier calls.</p>
    </Dialog>}
  </section>;
}

/** Review a saved system's tools again, with its saved key or a new one. */
function ReviewSystem({ agent, system, close, saved }: { agent: string; system: System; close: () => void; saved: () => Promise<void> }) {
  const [tools, setTools] = useState<ListedTool[]>();
  const [choices, setChoices] = useState<ReturnType<typeof initialChoices>>({});
  const [key, setKey] = useState('');
  const [done, setDone] = useState(false);
  const m = useMutation();
  const look = () => void m.run(async () => {
    const listed = (await savedTools(agent, system.id)).tools;
    setTools(listed); setChoices(initialChoices(listed, system.tools));
  });
  const save = () => void m.run(async () => {
    await reviewSystem(agent, system.id, { revision: system.revision, tools: chosen(choices), ...(key.trim() ? { key: key.trim() } : {}) });
    setDone(true); setKey(''); await saved();
  });
  return <Dialog drawer title={`Review ${system.name}`} close={close} busy={m.busy} status={m.status}
    footer={done ? <Button onClick={close}>Done</Button> : tools ? <Button primary disabled={m.busy} onClick={save}>Save review</Button> : <Button primary disabled={m.busy} onClick={look}>List its tools now</Button>}>
    {done ? <p role="status">Saved. Waiting approvals for this system ask again.</p> : <>
      <label className="ui-field">New key (leave empty to keep the saved one)<input type="password" autoComplete="off" value={key} onChange={event => setKey(event.target.value)} /></label>
      {tools ? <ReviewTools tools={tools} choices={choices} change={setChoices} held={system.held} />
        : <p className="ui-help">Cuckoo lists the system’s tools as they are now, so you can review what changed.</p>}
    </>}
  </Dialog>;
}
