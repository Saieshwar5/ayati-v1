import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import { agoText, appState, dayName, loadApps, type WorkspaceApp } from '../apps/apps-model';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { connectAccount } from './workspace-connections-model';

const problems: Record<string, string> = { reconnect: 'Needs a new sign-in', writes_paused: 'Writes paused', paused: 'Paused by Cuckoo' };

/** Settings › Connections, for admins: every app connected in the workspace,
 * whose it is, and Remove (schema 132). Agents' owners add and manage their own
 * on the agent's Connections tab. */
export function AllConnections() {
  const { data, error, refresh } = useResource(loadApps);
  const mutation = useMutation();
  const remove = (app: WorkspaceApp) => {
    const whose = app.owner ? app.owner.name : 'every agent that uses it';
    if (!confirm(`Remove ${app.name}? ${whose} stops using it at once.`)) return;
    void mutation.run(async () => {
      await connectAccount({ id: app.id, server: app.server, enabled: false, revision: app.revision ?? 0, agent: app.owner?.id });
      await refresh(); mutation.setStatus(`${app.name} removed.`);
    });
  };
  return <section className="settings-section" aria-label="Connections">
    <h2>Connections</h2>
    <p className="ui-help">Every app connected in this workspace. An agent’s owner adds apps on the agent’s own Connections tab, and each one belongs to that agent alone. You can remove any of them here.</p>
    {error && <p role="alert">{error} <Button onClick={() => void refresh().catch(() => {})}>Retry</Button></p>}
    {!data && !error && <p role="status">Loading connections…</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Removing…' : mutation.status}</p>
    {data && !data.apps.length && <p className="ui-empty">No apps are connected yet.</p>}
    {data && data.apps.length > 0 && <ul className="settings-rows">{data.apps.map(app => <li key={app.id} className="settings-row">
      <span><strong>{app.name}</strong>
        <small>{[app.account, app.owner ? null : 'Shared by the workspace', problems[appState(app)],
          app.connected && `connected ${dayName(app.connected.at)} by ${app.connected.by_name}`,
          app.last_used ? `used ${agoText(app.last_used.at)}` : 'not used yet'].filter(Boolean).join(' · ')}</small></span>
      {app.owner ? <a href={agentSectionUrl(app.owner.id, 'Connections')}>{app.owner.name}</a> : <span className="ui-help">Workspace</span>}
      {data.can_manage && <Button disabled={mutation.busy} aria-label={`Remove ${app.name}${app.owner ? ` from ${app.owner.name}` : ''}`} onClick={() => remove(app)}>Remove</Button>}
    </li>)}</ul>}
    <p className="ui-help"><a href={workspaceUrl('/?view=apps&manage=1')}>Older workspace connections and company systems</a></p>
  </section>;
}
