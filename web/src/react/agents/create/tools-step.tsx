import type { Mode, ToolConnection } from '../control-model';
import { connectorName, ToolAction } from '../tools-tab';

/** The connectors chosen during create, and any action made stricter. */
export type ToolChoices = Record<string, { on: boolean; modes: Record<string, Mode> }>;

/** Create step 4: connectors an admin added, all off at first, and Allow / Ask
 * first / Block per action, never looser than the admin's rule
 * (designs/agents-simplified 4). */
export function ToolsStep({ name, connections, choices, change }: {
  name: string; connections?: ToolConnection[]; choices: ToolChoices; change: (next: ToolChoices) => void;
}) {
  const display = name.trim() || 'this agent';
  if (!connections) return <p role="status">Loading connectors…</p>;
  if (!connections.length) return <p className="ui-empty">No connectors yet. An admin adds them in Settings → Connections; you can switch them on for {display} later.</p>;
  return <div className="agent-create-tools">
    <p className="agent-tools-note">Your admin set the loosest rule for each action. You can make {display} stricter, never looser. “Ask first” goes to its managers.</p>
    {connections.map(connection => {
      const chosen = choices[connection.id] ?? { on: false, modes: {} };
      const set = (next: typeof chosen) => change({ ...choices, [connection.id]: next });
      return <article key={connection.id} className="agent-tool" aria-label={connectorName(connection)}>
        <header>
          <span><strong>{connectorName(connection)}</strong><small>{connection.operations.length} {connection.operations.length === 1 ? 'action' : 'actions'}</small></span>
          <label className="ui-switch"><input type="checkbox" role="switch" checked={chosen.on} onChange={event => set({ ...chosen, on: event.target.checked })} />
            <span>Use {connectorName(connection)}</span></label>
        </header>
        {chosen.on && connection.operations.map(operation => <ToolAction key={operation.id} operation={operation} disabled={false}
          value={chosen.modes[operation.id] ?? operation.workspace_mode}
          change={mode => set({ ...chosen, modes: { ...chosen.modes, [operation.id]: mode } })} />)}
      </article>;
    })}
  </div>;
}
