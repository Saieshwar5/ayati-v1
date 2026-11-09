import type { ToolConnection } from '../control-model';
import { connectorName } from '../tools-tab';
import { meanings, type Meaning } from '../../settings/workspace-connections-model';
import { HealthTag } from '../../apps/app-marks';
import { NeedAnotherApp } from '../../apps/need-another-app';
import { appState } from '../../apps/apps-model';
import { currentWorkspace } from '../../../workspace-context';
import type { ReactNode } from 'react';
import type { ToolChoices } from './controls-step';

const order: Meaning[] = ['read', 'change', 'money', 'sends_outside', 'delete'];

/** Create step 2, Tools: Cuckoo's own tools (`builtIn`, designs/web-search) and the
 * apps the agent can use (designs/agent-built-skills 1), each with its health
 * (designs/connector-screens 5). Only apps an admin connected are listed, all off at
 * first. Switching one on makes it available; what the agent may do with it is
 * chosen in Controls. Admins can connect another app here without losing the draft. */
export function ConnectorsStep({ name, connections, choices, change, builtIn, refresh }: {
  name: string; connections?: ToolConnection[]; choices: ToolChoices; change: (next: ToolChoices) => void; builtIn?: ReactNode; refresh: () => void;
}) {
  const display = name.trim() || 'this agent';
  const paused = connections?.some(connection => appState(connection) === 'writes_paused');
  return <div className="agent-create-instructions">
    <div className="agent-create-main">
      <h3 className="agent-create-heading">What can {display} use?</h3>
      {builtIn}
      <h4 className="agent-group-label">Apps your admin added</h4>
      {!connections && <p role="status">Loading apps…</p>}
      {connections && !connections.length && <p className="ui-empty">No apps yet. Once an admin connects one, you can switch it on for {display} here or later on the Tools tab.</p>}
      {connections && connections.length > 0 && <ul className="agent-apps">{connections.map(connection => {
        const chosen = choices[connection.id] ?? { on: false, modes: {} };
        const kinds = order.filter(kind => connection.operations.some(operation => operation.meaning === kind));
        const label = connectorName(connection);
        return <li key={connection.id}>
          <span className="agent-app-text"><span className="agent-app-name"><strong>{label}</strong><HealthTag app={connection} /></span>
            <small>{[connection.account, `${connection.operations.length} ${connection.operations.length === 1 ? 'action' : 'actions'}`].filter(Boolean).join(' · ')}</small>
            {kinds.length > 0 && <span className="agent-app-kinds">{kinds.map(kind => <span key={kind} className={`agent-kind ${kind}`}>{meanings[kind]}</span>)}</span>}
            <small>{connection.operations.map(operation => operation.title).join(' · ')}</small>
          </span>
          <label className="ui-switch"><input type="checkbox" role="switch" checked={chosen.on}
            onChange={event => change({ ...choices, [connection.id]: { ...chosen, on: event.target.checked } })} />
            <span>Use {label}</span></label>
        </li>;
      })}</ul>}
      <NeedAnotherApp draft connected={refresh} />
    </div>
    <aside className="agent-create-side">
      <section className="agent-rules agent-apps-about" aria-label="About these apps">
        <h3>About these apps</h3>
        <p>Switching an app on gives {display} no permission by itself. What {display} may do is chosen in step 5, Controls.</p>
        {paused && <p><strong>Writes paused</strong> means one earlier action needs checking. {display} can still read.</p>}
        {currentWorkspace()?.role === 'Admin' && <p className="ui-help">You’re an admin: you can also connect an app here without losing this draft.</p>}
      </section>
    </aside>
  </div>;
}
