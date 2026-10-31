import { useCallback, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { effectLabel, loadControls, modes, saveRule, states, switchTool, type Action, type Mode, type ToolConnection, type ToolOperation } from './control-model';
import { ModePicker } from './mode-picker';
import type { Agent } from './model';

export const connectorName = (c: ToolConnection) => c.name || c.server;

/** Tools: the connectors an admin added, a switch per connector and a rule per
 * action that can only be stricter than the admin's (designs/agents-simplified 9).
 * Rules belong to the agent, not to one conversation. */
export function ToolsTab({ agent }: { agent: Agent }) {
  const load = useCallback(() => loadControls(agent.id), [agent.id]);
  const { data, error, refresh } = useResource(load);
  const mutation = useMutation();
  const [reviewing, review] = useState<string>();
  const editable = Boolean(data?.can_manage);
  const change = (work: () => Promise<unknown>, done: string) => void mutation.run(async () => {
    await work(); await refresh(); mutation.setStatus(done);
  });
  return <section className="agent-tools" aria-label="Tools">
    <p className="agent-tools-note">Your admin decides which tools exist and the loosest rule for each action. Here you can only make an action stricter. “Ask first” goes to {agent.name}’s managers.</p>
    {error && <p role="alert">{error} <Button onClick={() => void refresh().catch(() => {})}>Retry</Button></p>}
    {!data && !error && <p role="status">Loading tools…</p>}
    {data?.execution_available === false && <p role="status" className="ui-help">Business tools are not enabled for this workspace yet. Ask your operator to enable controller-managed execution.</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {data?.connections.map(connection => <article key={connection.id} className="agent-tool" aria-label={connectorName(connection)}>
      <header>
        <span><strong>{connectorName(connection)}</strong>
          <small>{[connection.account, `${connection.operations.length} ${connection.operations.length === 1 ? 'action' : 'actions'}`].filter(Boolean).join(' · ')}</small></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={connection.on} disabled={!editable || mutation.busy}
          onChange={event => change(() => switchTool(agent.id, connection.id, event.target.checked), event.target.checked ? `${agent.name} can use ${connectorName(connection)}.` : `${agent.name} no longer uses ${connectorName(connection)}.`)} />
          <span>Use {connectorName(connection)}</span></label>
      </header>
      {connection.on && connection.operations.map(operation => <ToolAction key={operation.id} operation={operation}
        disabled={!editable || mutation.busy}
        change={mode => change(() => saveRule(agent.id, { connection: connection.id, operation: operation.id, mode, revision: operation.revision }), `${operation.title}: ${modes[mode]}.`)} />)}
    </article>)}
    {data && !data.connections.length && <p className="ui-empty">No connectors yet. An admin adds them in Settings → Connections.</p>}
    {data && data.actions.length > 0 && <RecentActions actions={data.actions} review={review} />}
    {reviewing && <ApprovalDialog id={reviewing} close={() => { review(undefined); void refresh().catch(() => {}); }} />}
  </section>;
}

export function ToolAction({ operation, value = operation.mode, disabled, change }: {
  operation: ToolOperation; value?: Mode; disabled: boolean; change: (mode: Mode) => void;
}) {
  const limit = operation.workspace_mode === 'block' ? 'blocked by your admin' : `admin limit: ${modes[operation.workspace_mode]}`;
  return <div className="agent-tool-action">
    <span><strong>{operation.title}</strong>
      <small>{effectLabel(operation.effect)} · {limit}{operation.needs_other ? ' · someone other than the requester approves' : ''}</small></span>
    <ModePicker label={`${operation.title} rule`} value={value} ceiling={operation.workspace_mode} disabled={disabled} change={change} />
  </div>;
}

function RecentActions({ actions, review }: { actions: Action[]; review: (id: string) => void }) {
  return <section className="agent-tool-actions" aria-label="Recent tool actions">
    <h3>Recent tool actions</h3>
    <ul>{actions.slice(0, 10).map(action => <li key={action.id}>
      <span><strong>{action.title}</strong><small>{[action.conversation_name, action.requester_name && `asked by ${action.requester_name}`].filter(Boolean).join(' · ')}</small></span>
      <span className={`agent-tool-state ${action.state}`}>{action.decision === 'request_changes' ? 'Changes requested' : states[action.state] ?? action.state}</span>
      <Button onClick={() => review(action.id)}>Details</Button>
    </li>)}</ul>
  </section>;
}
