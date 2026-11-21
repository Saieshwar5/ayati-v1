import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { agoText, appState, dayName, loadApps, type WorkspaceApp } from '../apps/apps-model';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { connectAccount } from './workspace-connections-model';

const problems: Record<string, string> = { reconnect: 'Needs a new sign-in', writes_paused: 'Writes paused', paused: 'Paused by Cuckoo' };
export const hasProblem = (app: WorkspaceApp) => appState(app) !== 'ok';

/** Every app and MCP server connected in the workspace (schemas 132 and 133), and
 * Remove for an app's connection. Agents' owners add and manage their own on the
 * agent's Connections tab; an MCP server is removed there. */
export function useConnections() {
  const { data, error, refresh } = useResource(loadApps);
  const mutation = useMutation();
  const remove = (app: WorkspaceApp) => {
    if (!confirm(`Remove ${app.name}? ${app.owner?.name ?? 'Its agent'} stops using it at once.`)) return;
    void mutation.run(async () => {
      await connectAccount({ id: app.id, server: app.server, enabled: false, revision: app.revision ?? 0, agent: app.owner?.id });
      await refresh(); mutation.setStatus(`${app.name} removed.`);
    });
  };
  return { apps: data?.apps, canManage: data?.can_manage ?? false, error, refresh, remove, busy: mutation.busy, status: mutation.busy ? 'Removing…' : mutation.status };
}
export type Connections = ReturnType<typeof useConnections>;

/** One line per connection: the account, the agent it belongs to and how it is doing.
 * `named` adds the app's name, for connections listed outside their app. */
export function ConnectionRows({ apps, connections, named = false }: { apps: WorkspaceApp[]; connections: Connections; named?: boolean }) {
  return <ul className="settings-connections">{apps.map(app => <li key={app.id}>
    <span><strong>{named ? app.name : app.account || app.name}</strong>
      <small>{[named ? app.account : null, app.provider === 'system' ? 'MCP server' : null,
        app.connected && `connected ${dayName(app.connected.at)} by ${app.connected.by_name}`,
        app.last_used ? `used ${agoText(app.last_used.at)}` : 'not used yet'].filter(Boolean).join(' · ')}
        {hasProblem(app) && <b className="settings-connection-problem"> · {problems[appState(app)]}</b>}</small></span>
    {app.owner && <a href={agentSectionUrl(app.owner.id, 'Connections')}>{app.owner.name}</a>}
    {connections.canManage && app.provider !== 'system' && <Button disabled={connections.busy} aria-label={`Remove ${app.name}${app.owner ? ` from ${app.owner.name}` : ''}`} onClick={() => connections.remove(app)}>Remove</Button>}
  </li>)}</ul>;
}
