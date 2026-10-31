import { useRef } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { ConnectorSafety } from './connector-safety';
import { approveSystem, connectAccount, loadConnections } from './workspace-connections-model';
import './workspace-connections.css';

export function WorkspaceConnections() {
  const resource = useResource(loadConnections);
  const mutation = useMutation();
  const identities = useRef(new Map<string, string>());
  const data = resource.data;
  const servers = data ? [...data.servers, ...data.connections.filter(c => !data.servers.some(s => s.id === c.server)).map(c => c.definition), ...data.systems.filter(a => !data.servers.some(s => s.id === a.server) && !data.connections.some(c => c.server === a.server)).map(a => ({ id: a.server, name: a.server, account: null, operations: [] }))] : [];
  async function refresh() { await resource.refresh(); }
  const safety = (connection: string) => data?.safety.find(s => s.connection === connection);

  return <section className="workspace-connections" aria-label="Workspace connections">
    <header className="settings-section-heading"><div><h2>Connections</h2><p>Approve systems, connect accounts and set what agents may do with each action.</p></div>
      <Button disabled={mutation.busy} onClick={() => void refresh().catch(() => {})}>Refresh connections</Button></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {!data && !resource.error && <p role="status">Loading connections…</p>}
    <p role="status">{mutation.status}</p>
    {data && <>
      {data.catalog_error && <p role="alert">{data.catalog_error}</p>}
      <p className="ui-help">Only configured, reviewed remote MCP accounts are available here. Adding a new account or authentication method currently requires operator setup. Connecting an account gives no agent access until its owner switches it on.</p>
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
          mutation.setStatus(account?.enabled ? 'Disconnected. Agents lose access and its safety rules start over.' : 'Account verified. Set its safety rules below; agents switch it on from their Tools tab.');
        };
        return <article className="workspace-connection-row" key={server.id} aria-label={server.name}>
          <div className="connection-description"><h3>{server.name}</h3><p>{server.account ? `Account: ${server.account}` : 'Account identity supplied by the configured connection'}</p>
            <p>{approval?.approved ? 'Approved' : 'Not approved'} · {account?.enabled ? 'Connected' : 'Not connected'} · {server.operations.length} reviewed operations</p>
            {!configured && <p>Configuration unavailable. Existing access can be removed.</p>}
          </div>
          <div className="connection-row-actions">
            {(approval?.approved || account?.enabled) && <Button disabled={mutation.busy || (!account?.enabled && (!configured || !data.execution_available))} onClick={() => void mutation.run(connect)}>{account?.enabled ? 'Disconnect' : account ? 'Reconnect and review' : 'Connect account'}</Button>}
            <Button disabled={mutation.busy || (!configured && !approval?.approved)} onClick={() => void mutation.run(async () => {
              await approveSystem({ server: server.id, approved: !approval?.approved, revision: approval?.revision ?? 0 });
              await refresh(); mutation.setStatus(approval?.approved ? 'System approval removed. Its account and agent permissions are disabled.' : 'System approved. Connect its account next.');
            })}>{approval?.approved ? 'Remove approval' : 'Approve system'}</Button>
          </div>
          {account?.enabled && safety(account.id) && <ConnectorSafety name={server.name} safety={safety(account.id)!} refresh={refresh} />}
        </article>;
      })}</div>
      {!servers.length && <p className="ui-empty">No reviewed systems configured yet. Your operator can add a supported remote MCP account.</p>}
    </>}
  </section>;
}
