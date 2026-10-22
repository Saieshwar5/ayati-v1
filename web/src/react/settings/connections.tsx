import { useCallback, useState } from 'react';
import { api, post } from '../../types';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';

type Connection = { id: string; label: string; toolkit: string; status: string };
type Connections = { configured: boolean; setup_error?: string; connections: Connection[] };
type Config = { id: string; name: string; toolkit: string };
const loadConnections = () => api<Connections>('/connections');
export function LocalConnections() {
  const resource = useResource(loadConnections);
  const mutation = useMutation();
  const [connecting, setConnecting] = useState(false);
  return <section><header className="settings-section-heading"><div><h2>Connections</h2><p>Connect accounts, then assign operations separately in each agent’s Details.</p></div><Button disabled={mutation.busy} onClick={() => void resource.refresh().catch(() => {})}>Refresh connections</Button></header>
    <p role="status">{mutation.status || resource.error || resource.data?.setup_error || (!resource.data ? 'Loading connections…' : !resource.data.configured ? 'Configure COMPOSIO_API_KEY locally and restart Cuckoo to connect accounts.' : '')}</p>
    {resource.data?.configured && <Button primary onClick={() => setConnecting(true)}>Connect an account</Button>}
    {!resource.error && resource.data?.connections.map(connection => <article className="settings-connection" key={connection.id}><h3>{connection.label}</h3><p>{connection.toolkit} · {connection.status}</p>
      {!['REVOKED', 'LINK_UNCERTAIN'].includes(connection.status) && <div className="settings-connection-actions"><Button disabled={mutation.busy} onClick={() => void mutation.run(async () => { await post(`/connections/${connection.id}/refresh`); await resource.refresh(); })}>Refresh account</Button><Button disabled={mutation.busy} onClick={() => void mutation.run(async () => { await post(`/connections/${connection.id}/revoke`); setConnecting(false); await resource.refresh(); })}>Revoke workspace access</Button></div>}
      {connection.status === 'LINK_UNCERTAIN' && <p>Sign-in link creation was not acknowledged. Inspect connections in Composio before starting another flow.</p>}
    </article>)}
    {resource.data?.connections.length === 0 && <p>No connected accounts.</p>}
    {connecting && <ConnectAccount saved={resource.refresh} close={() => setConnecting(false)} />}
  </section>;
}
function ConnectAccount({ saved, close }: { saved: () => Promise<void>; close: () => void }) {
  const mutation = useMutation();
  const [draft, setDraft] = useState('');
  const [query, setQuery] = useState('');
  const resource = useResource(useCallback(() => api<{ mode?: string; configs: Config[] }>(`/connections/configs${query ? `?query=${encodeURIComponent(query)}` : ''}`), [query]));
  const data = resource.data;
  return <div className="settings-connect"><header className="settings-section-heading"><h3>Connect an account</h3><Button disabled={mutation.busy} onClick={close}>Close account setup</Button></header>
    {resource.error && <p role="alert">{resource.error}</p>}
    {data?.mode === 'consumer_mcp' && <form className="ui-form" onSubmit={event => { event.preventDefault(); if (query === draft) void resource.refresh().catch(() => {}); else setQuery(draft); }}><label className="ui-field">Discover Composio services<input disabled={mutation.busy} required maxLength={100} value={draft} onChange={event => setDraft(event.target.value)} placeholder="Describe a service or operation" /></label><Button type="submit" disabled={mutation.busy}>Discover services</Button></form>}
    {!resource.error && data && (data.configs.length ? <ServiceForm key={JSON.stringify(data.configs)} configs={data.configs} consumer={data.mode === 'consumer_mcp'} saved={saved} mutation={mutation} /> : <p>{data.mode === 'consumer_mcp' ? 'No matching services. Describe the service and operation to discover.' : 'Create the service’s auth configuration in your Composio project, then refresh.'}</p>)}
  </div>;
}
function ServiceForm({ configs, consumer, saved, mutation }: { configs: Config[]; consumer: boolean; saved: () => Promise<void>; mutation: ReturnType<typeof useMutation> }) {
  const [selected, setSelected] = useState(configs[0].id);
  const [label, setLabel] = useState('');
  const [linked, setLinked] = useState<{ connected?: boolean; redirect_url?: string }>();
  const [account, setAccount] = useState('');
  const chosen = configs.find(config => config.id === selected)!;
  if (linked) return linked.connected ? <p>Account selected. Assign operations in each agent’s Details.</p> : <><a className="ui-button" href={linked.redirect_url} target="_blank" rel="noopener noreferrer">Continue to service sign-in</a><p>After completing sign-in, use Refresh account on the saved connection. This sign-in link is private; do not share it.</p></>;
  return <form className="ui-form" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
    const result = await post<{ connected?: boolean; redirect_url?: string }>('/connections', { toolkit: chosen.toolkit, auth_config_id: selected, label, ...(account ? { account_id: account } : {}) });
    if (!result.connected && !result.redirect_url) throw new Error('Sign-in URL was not acknowledged. Inspect saved connections before retrying.');
    setLinked(result); await saved();
  }); }}><fieldset className="ui-fieldset" disabled={mutation.busy}>
    <label className="ui-field">Service<select value={selected} onChange={event => { setSelected(event.target.value); setAccount(''); }}>{configs.map(config => <option key={config.id} value={config.id}>{config.toolkit} · {config.name}</option>)}</select></label>
    <label className="ui-field">Account label<input required maxLength={100} value={label} onChange={event => setLabel(event.target.value)} placeholder="Work Gmail" /></label>
    {consumer && <AccountChoices key={chosen.toolkit} toolkit={chosen.toolkit} selected={account} select={setAccount} />}
    <Button type="submit" primary disabled={mutation.busy}>{consumer ? 'Use selected account or create sign-in link' : 'Create sign-in link'}</Button>
  </fieldset><p role="status">{mutation.status}</p></form>;
}
function AccountChoices({ toolkit, selected, select }: { toolkit: string; selected: string; select: (id: string) => void }) {
  const resource = useResource(useCallback(() => api<{ accounts: { id: string; alias?: string; status: string }[] }>(`/connections/accounts?toolkit=${encodeURIComponent(toolkit)}`), [toolkit]));
  return <><label className="ui-field">Composio account<select value={selected} onChange={event => select(event.target.value)} disabled={!resource.data || !!resource.error}><option value="">Connect another account</option>{resource.data?.accounts.filter(account => account.status === 'ACTIVE').map(account => <option key={account.id} value={account.id}>{account.alias || account.id}</option>)}</select></label>{resource.error && <p role="alert">{resource.error}</p>}</>;
}
