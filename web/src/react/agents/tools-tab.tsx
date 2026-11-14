import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { OutcomeDialog } from '../approvals/outcome-dialog';
import { HealthTag } from '../apps/app-marks';
import { NeedAnotherApp } from '../apps/need-another-app';
import { appState } from '../apps/apps-model';
import { workspaceUrl } from '../../workspace-context';
import { meanings } from '../settings/workspace-connections-model';
import { effectLabel, loadControls, modes, saveRule, states, switchTool, type Action, type Mode, type ToolConnection, type ToolOperation } from './control-model';
import { ModePicker } from './mode-picker';
import type { Agent } from './model';
import { WebSearchCard } from './web-search';
import { ComputerToolsCard } from './computer/computer-controls';
import { SystemsSection } from './systems/systems-section';

export const connectorName = (c: ToolConnection) => c.name || c.server;

/** Tools: Cuckoo's own tools first (web search, designs/web-search, and the agent's
 * computer controls when it has one, designs/agent-computers 8), then the
 * connectors an admin added with their health (designs/connector-screens 6), a
 * switch per connector and a rule per action that can only be stricter than the
 * admin's (designs/agents-simplified 9). Rules belong to the agent, not to one
 * conversation. `focus` ("connection/operation", from a suggestion in Activity ›
 * Review) marks one action and brings it into view. */
export function ToolsTab({ agent, webSearch = false, computers = false, saved, focus }: { agent: Agent; webSearch?: boolean; computers?: boolean; saved?: () => Promise<void>; focus?: string }) {
  const load = useCallback(() => loadControls(agent.id), [agent.id]);
  const { data, error, refresh } = useResource(load);
  const section = useRef<HTMLElement>(null);
  const loaded = Boolean(data);
  useEffect(() => {
    if (!loaded || !focus) return;
    const target = [...(section.current?.querySelectorAll<HTMLElement>('[data-action]') ?? [])].find(node => node.dataset.action === focus);
    target?.scrollIntoView?.({ block: 'center' });
  }, [loaded, focus]);
  const mutation = useMutation();
  const [reviewing, review] = useState<string>();
  const [checking, check] = useState<string>();
  const editable = Boolean(data?.can_manage);
  const change = (work: () => Promise<unknown>, done: string) => void mutation.run(async () => {
    await work(); await refresh(); mutation.setStatus(done);
  });
  return <section ref={section} className="agent-tools" aria-label="Tools">
    {webSearch && saved && <WebSearchCard agent={agent} saved={saved} />}
    {computers && saved && <ComputerToolsCard agent={agent} saved={saved} />}
    {(webSearch || computers) && <h3 className="agent-group-label">Apps your admin added</h3>}
    <p className="agent-tools-note">Your admin decides which tools exist and the loosest rule for each action. Here you can only make an action stricter. “Ask first” goes to {agent.name}’s managers.</p>
    {error && <p role="alert">{error} <Button onClick={() => void refresh().catch(() => {})}>Retry</Button></p>}
    {!data && !error && <p role="status">Loading tools…</p>}
    {data?.execution_available === false && <p role="status" className="ui-help">Business tools are not enabled for this workspace yet. Ask your operator to enable controller-managed execution.</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {data?.connections.filter(connection => connection.provider !== 'system').map(connection => <article key={connection.id} className="agent-tool" aria-label={connectorName(connection)}>
      <header>
        <span><span className="agent-app-name"><strong>{connectorName(connection)}</strong><HealthTag app={connection} /></span>
          <small>{[connection.account, `${connection.operations.length} ${connection.operations.length === 1 ? 'action' : 'actions'}`].filter(Boolean).join(' · ')}</small>
          <Health connection={connection} agent={agent.name} admin={Boolean(data.can_manage_connections)}
            check={data.actions.find(action => action.connection === connection.id && action.state === 'uncertain')?.id} open={check} /></span>
        <label className="ui-switch"><input type="checkbox" role="switch" checked={connection.on} disabled={!editable || mutation.busy}
          onChange={event => change(() => switchTool(agent.id, connection.id, event.target.checked), event.target.checked ? `${agent.name} can use ${connectorName(connection)}.` : `${agent.name} no longer uses ${connectorName(connection)}.`)} />
          <span>Use {connectorName(connection)}</span></label>
      </header>
      {connection.on && connection.operations.map(operation => <ToolAction key={operation.id} operation={operation}
        disabled={!editable || mutation.busy} action={`${connection.id}/${operation.id}`} highlighted={focus === `${connection.id}/${operation.id}`}
        note={focus === `${connection.id}/${operation.id}` ? 'Suggested in Activity › Review: approved every time it asked.' : undefined}
        change={mode => change(() => saveRule(agent.id, { connection: connection.id, operation: operation.id, mode, revision: operation.revision }), `${operation.title}: ${modes[mode]}.`)} />)}
    </article>)}
    {data && !data.connections.some(connection => connection.provider !== 'system') && <><p className="ui-empty">No apps yet. Once an admin connects one, it can be switched on here.</p>
      <NeedAnotherApp connected={() => void refresh().catch(() => {})} /></>}
    <SystemsSection agent={agent} />
    {data && data.actions.length > 0 && <RecentActions actions={data.actions} review={action => action.state === 'uncertain' ? check(action.id) : review(action.id)} />}
    {reviewing && <ApprovalDialog id={reviewing} close={() => { review(undefined); void refresh().catch(() => {}); }} />}
    {checking && <OutcomeDialog id={checking} close={() => { check(undefined); void refresh().catch(() => {}); }} />}
  </section>;
}

export function ToolAction({ operation, value = operation.mode, disabled, change, note, action, highlighted = false }: {
  operation: ToolOperation; value?: Mode; disabled: boolean; change: (mode: Mode) => void; note?: string; action?: string; highlighted?: boolean;
}) {
  const limit = operation.workspace_mode === 'block' ? 'blocked by your admin' : `admin limit: ${modes[operation.workspace_mode]}`;
  return <div className={`agent-tool-action${highlighted ? ' agent-tool-action-suggested' : ''}`} data-action={action}>
    <span><strong>{operation.title}</strong>
      <small>{operation.meaning ? meanings[operation.meaning] : effectLabel(operation.effect)} · {limit}{operation.needs_other ? ' · someone other than the requester approves' : ''}</small>
      {note && <small className="agent-tool-note">{note}</small>}</span>
    <ModePicker label={`${operation.title} rule`} value={value} ceiling={operation.workspace_mode} disabled={disabled} change={change} />
  </div>;
}

/** What a connection's health means for this agent, with the fix where there is one. */
function Health({ connection, agent, admin, check, open }: { connection: ToolConnection; agent: string; admin: boolean; check?: string; open: (id: string) => void }) {
  const state = appState(connection), name = connectorName(connection);
  if (state === 'writes_paused') return <small className="agent-tool-health">{agent} can read. Writing waits until someone checks one earlier action.
    {check && <> <button type="button" className="agent-tool-link" onClick={() => open(check)}>Check it</button></>}</small>;
  if (state === 'reconnect') return <small className="agent-tool-health">The sign-in expired. {agent} can’t use {name} until an admin reconnects it.
    {admin && <> <a className="agent-tool-link" href={workspaceUrl('/?view=apps&manage=1')}>Reconnect on Apps</a></>}</small>;
  if (state === 'paused') return <small className="agent-tool-health">Cuckoo paused {name} for a short while. Nothing is sent to it.</small>;
  return null;
}

function RecentActions({ actions, review }: { actions: Action[]; review: (action: Action) => void }) {
  return <section className="agent-tool-actions" aria-label="Recent tool actions">
    <h3>Recent tool actions</h3>
    <ul>{actions.slice(0, 10).map(action => <li key={action.id}>
      <span><strong>{action.title}</strong><small>{[action.conversation_name, action.requester_name && `asked by ${action.requester_name}`].filter(Boolean).join(' · ')}</small></span>
      <span className={`agent-tool-state ${action.state}`}>{action.decision === 'request_changes' ? 'Changes requested' : states[action.state] ?? action.state}</span>
      <Button onClick={() => review(action)}>{action.state === 'uncertain' ? 'Check' : 'Details'}</Button>
    </li>)}</ul>
  </section>;
}
