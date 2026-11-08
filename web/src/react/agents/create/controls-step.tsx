import type { Mode, ToolConnection } from '../control-model';
import { connectorName, ToolAction } from '../tools-tab';

/** The connectors chosen during create, and any action made stricter. */
export type ToolChoices = Record<string, { on: boolean; modes: Record<string, Mode> }>;

/** Create step 5, last: Allow / Ask first / Block for every action of the apps
 * chosen in Connectors, never looser than the admin's rule
 * (designs/agent-built-skills 4). Cuckoo enforces these before every action. */
export function ControlsStep({ name, connections, choices, change }: {
  name: string; connections?: ToolConnection[]; choices: ToolChoices; change: (next: ToolChoices) => void;
}) {
  const display = name.trim() || 'this agent';
  const chosen = connections?.filter(connection => choices[connection.id]?.on) ?? [];
  if (!connections) return <p role="status">Loading connectors…</p>;
  return <div className="agent-create-tools">
    <p className="agent-tools-note">Your admin set the loosest rule for each action. You can make {display} stricter, never looser. “Ask first” goes to its managers. Cuckoo checks these before every action.</p>
    {!chosen.length && <p className="ui-empty">No apps chosen, so {display} can’t act in any app. You can switch apps on later on the Tools tab.</p>}
    {chosen.map(connection => {
      const choice = choices[connection.id];
      return <article key={connection.id} className="agent-tool" aria-label={connectorName(connection)}>
        <header><span><strong>{connectorName(connection)}</strong>
          <small>{connection.operations.length} {connection.operations.length === 1 ? 'action' : 'actions'}</small></span></header>
        {connection.operations.map(operation => <ToolAction key={operation.id} operation={operation} disabled={false} value={choice.modes[operation.id] ?? operation.workspace_mode}
          change={mode => change({ ...choices, [connection.id]: { ...choice, modes: { ...choice.modes, [operation.id]: mode } } })} />)}
      </article>;
    })}
  </div>;
}
