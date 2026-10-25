import { useRef, useState } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { ConnectionAccess } from './connection-access';
import { approveSystem, connectAccount, loadConnections, saveCreator, type Creator } from './workspace-connections-model';
import './workspace-connections.css';

export function WorkspaceConnections() {
  const resource = useResource(loadConnections);
  const mutation = useMutation();
  const [selected, select] = useState('');
  const identities = useRef(new Map<string, string>());
  const data = resource.data;
  const connection = data?.connections.find(c => c.id === selected && c.enabled);
  const servers = data ? [...data.servers, ...data.connections.filter(c => !data.servers.some(s => s.id === c.server)).map(c => c.definition), ...data.systems.filter(a => !data.servers.some(s => s.id === a.server) && !data.connections.some(c => c.server === a.server)).map(a => ({ id: a.server, name: a.server, account: null, operations: [] }))] : [];
  async function refresh() { await resource.refresh(); }

  return <section className="workspace-connections" aria-label="Workspace connections">
    <header className="settings-section-heading"><div><h2>Connections</h2><p>Approve systems, verify accounts and choose who can configure agent access.</p></div>
      <Button disabled={mutation.busy || Boolean(selected)} onClick={() => void refresh().catch(() => {})}>Refresh connections</Button></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading connections…</p>}
    <p role="status">{mutation.status}</p>
    {data && <>
      {data.catalog_error && <p role="alert">{data.catalog_error}</p>}
      <p className="ui-help">Only configured, reviewed remote MCP accounts are available here. Adding a new account or authentication method currently requires operator setup. Connecting an account gives agents no automatic access.</p>
      {!data.execution_available && <p role="status">Business tools are unavailable with the current worker setup. You can still remove access or disconnect accounts.</p>}
      <div aria-label="Business systems">{servers.map(server => {
        const approval = data.systems.find(s => s.server === server.id);
        const account = data.connections.find(c => c.server === server.id);
        const configured = data.servers.some(s => s.id === server.id);
        const connect = async () => {
          let id = account?.id ?? identities.current.get(server.id);
          if (!id) { id = crypto.randomUUID(); identities.current.set(server.id, id); }
          await connectAccount({ id, server: server.id, enabled: !account?.enabled, revision: account?.revision ?? 0 });
          await refresh();
          mutation.setStatus(account?.enabled ? 'Disconnected. Existing permissions and delegated access are blocked.' : 'Account verified. Configure access before assigning agent permissions.');
        };
        return <article className="workspace-connection-row" key={server.id} aria-label={server.name}>
          <div className="connection-description"><h3>{server.name}</h3><p>{server.account ? `Account: ${server.account}` : 'Account identity supplied by the configured connection'}</p>
            <p>{approval?.approved ? 'Approved' : 'Not approved'} · {account?.enabled ? 'Connected' : 'Not connected'} · {server.operations.length} reviewed operations</p>
            {!configured && <p>Configuration unavailable. Existing access can be removed.</p>}
          </div>
          <div className="connection-row-actions">
            {account?.enabled && <Button disabled={mutation.busy} onClick={() => select(account.id)}>Manage access</Button>}
            {(approval?.approved || account?.enabled) && <Button disabled={mutation.busy || (!account?.enabled && (!configured || !data.execution_available))} onClick={() => void mutation.run(connect)}>{account?.enabled ? 'Disconnect' : account ? 'Reconnect and review' : 'Connect account'}</Button>}
            <Button disabled={mutation.busy || (!configured && !approval?.approved)} onClick={() => void mutation.run(async () => {
              await approveSystem({ server: server.id, approved: !approval?.approved, revision: approval?.revision ?? 0 });
              await refresh(); mutation.setStatus(approval?.approved ? 'System approval removed. Its account and agent permissions are disabled.' : 'System approved. Connect its account next.');
            })}>{approval?.approved ? 'Remove approval' : 'Approve system'}</Button>
          </div>
        </article>;
      })}</div>
      {!servers.length && <p className="ui-empty">No reviewed systems configured yet. Your operator can add a supported remote MCP account.</p>}
      <section className="connection-creators" aria-label="Workspace agent creators"><h3>Agent creators</h3><p className="ui-help">Allow selected people to create workspace agents. Tool access is delegated separately for each connected account. Turning this off prevents new workspace agents; it does not remove existing agents.</p>
        {data.people.filter(p => p.role === 'User').map(person => <CreatorRow key={`${person.id}/${person.revision}`} person={person} busy={mutation.busy}
          save={enabled => mutation.run(async () => { await saveCreator(person, enabled); await refresh(); mutation.setStatus('Agent creation permission saved.'); })} />)}
        {!data.people.some(p => p.role === 'User') && <p>Owners already have creation authority. Add workspace Users to delegate it.</p>}
      </section>
      {connection && <ConnectionAccess key={connection.id} connection={connection} data={data} close={() => select('')} refresh={refresh} />}
    </>}
  </section>;
}
function CreatorRow({ person, busy, save }: { person: Creator; busy: boolean; save: (value: boolean) => Promise<void> }) {
  const [enabled, setEnabled] = useState(person.can_create);
  return <form className="connection-creator-row" onSubmit={event => { event.preventDefault(); void save(enabled); }}>
    <label className="ui-check"><input type="checkbox" aria-label={`Allow ${person.name} to create workspace agents`} checked={enabled} disabled={busy} onChange={event => setEnabled(event.target.checked)} />{person.name}</label>
    <span>{person.can_create ? 'May create workspace agents' : 'Private agents only'}</span>
    <Button type="submit" disabled={busy || enabled === person.can_create} aria-label={`Save agent creation for ${person.name}`}>Save</Button>
  </form>;
}
