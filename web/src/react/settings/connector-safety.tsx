import { useMutation } from '../requests';
import { ModePicker } from '../agents/mode-picker';
import { effectLabel, modes, type Mode } from '../agents/control-model';
import { saveConnectorRule, type Safety } from './workspace-connections-model';

const names = (agents: string[]) => agents.length ? `used by ${agents.length > 2 ? `${agents.slice(0, 2).join(', ')} and ${agents.length - 2} more` : agents.join(' and ')}` : 'not used by any agent yet';

/** The workspace rule for each action of one connected account (designs/access-model 3).
 * It is the loosest any agent may use; agent owners can only make it stricter. */
export function ConnectorSafety({ name, safety, refresh }: { name: string; safety: Safety; refresh: () => Promise<void> }) {
  const mutation = useMutation();
  const save = (operation: Safety['operations'][number], mode: Mode, needs_other: boolean) => void mutation.run(async () => {
    await saveConnectorRule({ connection: safety.connection, operation: operation.id, mode, needs_other: mode === 'ask' && needs_other, revision: operation.revision });
    await refresh();
    mutation.setStatus(`${operation.title}: ${modes[mode]}${mode === 'ask' && needs_other ? ', someone else approves' : ''}.`);
  });
  return <section className="connector-safety" aria-label={`Safety for ${name}`}>
    <div className="connector-safety-note"><strong>Safety for every agent using {name}</strong>
      <span>Agent owners can make their agent stricter, never looser. “Ask first” goes to that agent’s managers. {name} is {names(safety.agents)}.</span></div>
    <ul>{safety.operations.map(operation => <li key={operation.id}>
      <span><strong>{operation.title}</strong><small>{effectLabel(operation.effect)}{operation.default ? ' · safe default' : ''}</small>
        {operation.mode === 'ask' && <label className="ui-check"><input type="checkbox" checked={operation.needs_other} disabled={mutation.busy}
          onChange={event => save(operation, 'ask', event.target.checked)} />Someone other than the requester must approve</label>}</span>
      <ModePicker label={`${operation.title} workspace rule`} value={operation.mode} disabled={mutation.busy} change={mode => save(operation, mode, operation.needs_other)} />
    </li>)}</ul>
    <p className="ui-help">New connectors start safe: reading is allowed, changes ask first.</p>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
  </section>;
}
