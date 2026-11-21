import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { OutcomeDialog } from '../approvals/outcome-dialog';
import { HealthTag } from '../apps/app-marks';
import { appState, connectApp, type CatalogApp } from '../apps/apps-model';
import { CatalogDialog } from '../apps/catalog-dialog';
import { ConnectDialog } from '../apps/connect-dialog';
import { connectAccount, meanings, saveConnectorRule } from '../settings/workspace-connections-model';
import { effectLabel, loadControls, modes, states, type Action, type Mode, type ToolConnection, type ToolOperation } from './control-model';
import { ModePicker } from './mode-picker';
import type { Agent } from './model';
import { workspaceUrl } from '../../workspace-context';

export const connectorName = (c: ToolConnection) => c.name || c.server;

/** The Apps group of an agent's connections (schemas 132 and 133). Every app
 * here is this agent's own: its owner adds it from the catalog and signs in on
 * the app's page, changes its one rule per action, signs in again when the
 * sign-in runs out and disconnects it. `children` are the other groups (MCP
 * servers, computers, built-in tools), shown before the recent actions. `focus`
 * ("connection/operation", from a suggestion in Activity › Review) marks one
 * action and brings it into view.
 * `only` shows one app by its connection id, without the group's heading, the
 * other groups or the recent actions (the agent page's panel for one app);
 * `adding` opens the catalog at once; `changed` is told after every change. */
export function AppConnections({ agent, focus, children, only, adding: startAdding = false, changed }: {
  agent: Agent; focus?: string; children?: ReactNode; only?: string; adding?: boolean; changed?: () => void;
}) {
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
  const [adding, setAdding] = useState(startAdding);
  const [connecting, setConnecting] = useState<CatalogApp>();
  const editable = Boolean(data?.can_manage);
  // Back from the app's own sign-in page.
  const [returned] = useState(() => { const query = new URLSearchParams(window.location.search); return query.has('connected') ? 'connected' : query.has('connect_error') ? 'failed' : ''; });
  const change = (work: () => Promise<unknown>, done: string) => void mutation.run(async () => {
    await work(); await refresh(); mutation.setStatus(done); changed?.();
  });
  const disconnect = (connection: ToolConnection) => {
    if (!confirm(`Disconnect ${connectorName(connection)} from ${agent.name}? ${agent.name} stops using it at once.`)) return;
    change(() => connectAccount({ id: connection.id, server: connection.server, enabled: false, revision: connection.revision, agent: agent.id }), `${connectorName(connection)} disconnected.`);
  };
  const reconnect = (connection: ToolConnection) => void mutation.run(async () => {
    const result = await connectApp(connection.server, crypto.randomUUID(), agent.id);
    if (result.redirect_url) location.assign(result.redirect_url); else await refresh();
  });
  // A connected system (an MCP server) has its own group.
  const apps = data?.connections.filter(connection => connection.provider !== 'system' && (!only || connection.id === only)) ?? [];
  const finish = () => { setConnecting(undefined); setAdding(false); };
  return <section ref={section} className="agent-tools" aria-label={only ? 'App' : 'Connections'}>
    {!only && <h3 className="agent-group-label agent-group-head">Apps{editable && <Button onClick={() => setAdding(true)}>＋ Add app</Button>}</h3>}
    {returned === 'connected' && <p className="ui-status" role="status">The app is connected.</p>}
    {returned === 'failed' && <p role="alert">The sign-in did not finish. Add the app again.</p>}
    {error && <p role="alert">{error} <Button onClick={() => void refresh().catch(() => {})}>Retry</Button></p>}
    {!data && !error && <p role="status">Loading apps…</p>}
    {data?.execution_available === false && <p role="status" className="ui-help">Apps can’t run on this server yet.</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {apps.map(connection => <article key={connection.id} className="agent-tool" aria-label={connectorName(connection)}>
      <header>
        <span><span className="agent-app-name"><strong>{connectorName(connection)}</strong><HealthTag app={connection} /></span>
          <small>{[connection.account, `${connection.operations.length} ${connection.operations.length === 1 ? 'action' : 'actions'}`].filter(Boolean).join(' · ')}</small>
          <Health connection={connection} agent={agent.name} reconnect={editable ? () => reconnect(connection) : undefined}
            check={data?.actions.find(action => action.connection === connection.id && action.state === 'uncertain')?.id} open={check} /></span>
        {editable && <Button disabled={mutation.busy} onClick={() => disconnect(connection)}>Disconnect</Button>}
      </header>
      {connection.operations.map(operation => <ToolAction key={operation.id} operation={operation}
        disabled={!editable || mutation.busy} action={`${connection.id}/${operation.id}`} highlighted={focus === `${connection.id}/${operation.id}`}
        note={focus === `${connection.id}/${operation.id}` ? 'Suggested in Activity › Insights: approved every time it asked.' : undefined}
        change={mode => change(() => saveConnectorRule({ connection: connection.id, operation: operation.id, mode, needs_other: operation.needs_other, revision: operation.rule_revision ?? 0 }), `${operation.title}: ${modes[mode]}.`)} />)}
    </article>)}
    {data && !apps.length && <p className="ui-empty">{only ? 'This app is no longer connected.' : 'No apps yet.'}</p>}
    {!only && children}
    {!only && data && data.actions.length > 0 && <RecentActions actions={data.actions} review={action => action.state === 'uncertain' ? check(action.id) : review(action.id)} />}
    {adding && !connecting && <CatalogDialog agent={agent.id} close={() => setAdding(false)} connect={setConnecting} />}
    {connecting && <ConnectDialog app={connecting} agent={agent} close={finish}
      connected={() => { finish(); void refresh().catch(() => {}); changed?.(); }} />}
    {reviewing && <ApprovalDialog id={reviewing} close={() => { review(undefined); void refresh().catch(() => {}); }} />}
    {checking && <OutcomeDialog id={checking} close={() => { check(undefined); void refresh().catch(() => {}); }} />}
  </section>;
}

/** One action of an app with its rule: Allow, Ask first or Block. */
export function ToolAction({ operation, disabled, change, note, action, highlighted = false }: {
  operation: ToolOperation; disabled: boolean; change: (mode: Mode) => void; note?: string; action?: string; highlighted?: boolean;
}) {
  return <div className={`agent-tool-action${highlighted ? ' agent-tool-action-suggested' : ''}`} data-action={action}>
    <span><strong>{operation.title}</strong>
      <small>{operation.meaning ? meanings[operation.meaning] : effectLabel(operation.effect)}{operation.needs_other ? ' · someone other than the requester approves' : ''}</small>
      {note && <small className="agent-tool-note">{note}</small>}</span>
    <ModePicker label={`${operation.title} rule`} value={operation.workspace_mode} disabled={disabled} change={change} />
  </div>;
}

/** What a connection's health means for this agent, with the fix where there is one. */
function Health({ connection, agent, check, open, reconnect }: { connection: ToolConnection; agent: string; check?: string; open: (id: string) => void; reconnect?: () => void }) {
  const state = appState(connection), name = connectorName(connection);
  if (state === 'writes_paused') return <small className="agent-tool-health">{agent} can read. Writing waits until someone checks one earlier action.
    {check && <> <button type="button" className="agent-tool-link" onClick={() => open(check)}>Check it</button></>}</small>;
  if (state === 'reconnect') return <small className="agent-tool-health">The sign-in expired. {agent} can’t use {name} until someone signs in again.
    {reconnect && <> <button type="button" className="agent-tool-link" onClick={reconnect}>Sign in again</button></>}</small>;
  if (state === 'paused') return <small className="agent-tool-health">Cuckoo paused {name} for a short while. Nothing is sent to it.</small>;
  return null;
}

function RecentActions({ actions, review }: { actions: Action[]; review: (action: Action) => void }) {
  return <section className="agent-tool-actions" aria-label="Recent tool actions">
    <h3>Recent tool actions</h3>
    <ul>{actions.slice(0, 10).map(action => <li key={action.id}>
      <span><strong>{action.title}</strong><small>{[action.conversation_name, action.requester_name && `asked by ${action.requester_name}`].filter(Boolean).join(' · ')}</small></span>
      <span className={`agent-tool-state ${action.state}`}>{action.decision === 'request_changes' ? 'Changes requested' : states[action.state] ?? action.state}</span>
      {/* A waiting action is decided in Activity or its conversation, never on the agent's page. */}
      {action.state === 'pending' ? <a className="ui-button" href={workspaceUrl('/?view=activity&tab=needs')}>Review in Activity</a>
        : <Button onClick={() => review(action)}>{action.state === 'uncertain' ? 'Check' : 'Details'}</Button>}
    </li>)}</ul>
  </section>;
}
