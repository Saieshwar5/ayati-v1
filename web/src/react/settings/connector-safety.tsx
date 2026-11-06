import { useMutation } from '../requests';
import { ModePicker } from '../agents/mode-picker';
import { effectLabel, modes, type Mode } from '../agents/control-model';
import { meanings, saveConnectorRule, saveMeaning, type Meaning, type Safety } from './workspace-connections-model';

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
  const review = (operation: Safety['operations'][number], meaning: Meaning) => void mutation.run(async () => {
    await saveMeaning({ connection: safety.connection, operation: operation.id, meaning, revision: operation.meaning_revision });
    await refresh();
    mutation.setStatus(`${operation.title}: ${meanings[meaning]}.`);
  });
  return <section className="connector-safety" aria-label={`Safety for ${name}`}>
    <div className="connector-safety-note"><strong>Safety for every agent using {name}</strong>
      <span>Agent owners can make their agent stricter, never looser. “Ask first” goes to that agent’s managers. {name} is {names(safety.agents)}.</span></div>
    <ul>{safety.operations.map(operation => <li key={operation.id}>
      <span><strong>{operation.title}</strong><small>{meanings[operation.meaning] ?? effectLabel(operation.effect)}{operation.meaning_reviewed ? '' : ' (suggested)'}{operation.default ? ' · safe default' : ''}</small>
        <label className="connector-meaning">What it does<select value={operation.meaning} disabled={mutation.busy}
          onChange={event => review(operation, event.target.value as Meaning)}>
          {(Object.keys(meanings) as Meaning[]).filter(m => (m === 'read') === (operation.effect === 'read')).map(m => <option key={m} value={m}>{meanings[m]}</option>)}
        </select></label>
        {operation.mode === 'ask' && <label className="ui-check"><input type="checkbox" checked={operation.needs_other} disabled={mutation.busy}
          onChange={event => save(operation, 'ask', event.target.checked)} />Someone other than the requester must approve</label>}</span>
      <ModePicker label={`${operation.title} workspace rule`} value={operation.mode} disabled={mutation.busy} change={mode => save(operation, mode, operation.needs_other)} />
    </li>)}</ul>
    <p className="ui-help">New actions start safe: reading is allowed, deleting is blocked, everything else asks first. Company rules can cover every action of one kind.</p>
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
  </section>;
}
