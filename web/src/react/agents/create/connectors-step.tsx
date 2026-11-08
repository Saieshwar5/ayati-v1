import type { ToolConnection } from '../control-model';
import { connectorName } from '../tools-tab';
import { meanings, type Meaning } from '../../settings/workspace-connections-model';
import type { ReactNode } from 'react';
import type { ToolChoices } from './controls-step';

const order: Meaning[] = ['read', 'change', 'money', 'sends_outside', 'delete'];

/** Create step 2, Tools: Cuckoo's own tools (`builtIn`, designs/web-search) and the
 * apps the agent can use (designs/agent-built-skills 1). Only connectors an admin
 * added are listed, all off at first. Switching one on makes it available; what
 * the agent may do with it is chosen in Controls. */
export function ConnectorsStep({ name, connections, choices, change, builtIn }: {
  name: string; connections?: ToolConnection[]; choices: ToolChoices; change: (next: ToolChoices) => void; builtIn?: ReactNode;
}) {
  const display = name.trim() || 'this agent';
  return <div className="agent-create-instructions">
    <div className="agent-create-main">
      <h3 className="agent-create-heading">What can {display} use?</h3>
      {builtIn}
      <h4 className="agent-group-label">Apps your admin added</h4>
      {!connections && <p role="status">Loading connectors…</p>}
      {connections && !connections.length && <p className="ui-empty">No connectors yet. An admin adds them in Settings → Connections; you can switch them on for {display} later.</p>}
      {connections && connections.length > 0 && <ul className="agent-apps">{connections.map(connection => {
        const chosen = choices[connection.id] ?? { on: false, modes: {} };
        const kinds = order.filter(kind => connection.operations.some(operation => operation.meaning === kind));
        const label = connectorName(connection);
        return <li key={connection.id}>
          <span className="agent-app-text"><strong>{label}</strong>
            <small>{[connection.account, `${connection.operations.length} ${connection.operations.length === 1 ? 'action' : 'actions'}`].filter(Boolean).join(' · ')}</small>
            {kinds.length > 0 && <span className="agent-app-kinds">{kinds.map(kind => <span key={kind} className={`agent-kind ${kind}`}>{meanings[kind]}</span>)}</span>}
            <small>{connection.operations.map(operation => operation.title).join(' · ')}</small>
          </span>
          <label className="ui-switch"><input type="checkbox" role="switch" checked={chosen.on}
            onChange={event => change({ ...choices, [connection.id]: { ...chosen, on: event.target.checked } })} />
            <span>Use {label}</span></label>
        </li>;
      })}</ul>}
      <p className="ui-help">Need another app or MCP server? Admins add them in Settings → Connections.</p>
    </div>
    <aside className="agent-create-side">
      <section className="agent-rules" aria-label="How setup works">
        <h3>How {display} gets set up</h3>
        <ol className="agent-setup-steps">
          <li><strong>Tools</strong> What {display} can use.</li>
          <li><strong>Skills</strong> How it does the job, with these apps.</li>
          <li><strong>People</strong> Who talks to it and who approves.</li>
          <li><strong>Controls</strong> What it may do with each app. Cuckoo enforces it.</li>
        </ol>
        <p className="ui-help">Switching an app on gives {display} no permission by itself.</p>
      </section>
    </aside>
  </div>;
}
