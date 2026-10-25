import { useState } from 'react';
import { LocalConnections } from './connections';
import { WorkspaceConnections } from './workspace-connections';
import { api, post } from '../../types';
import { workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';

type Setup = { provider: string; model: string; execution: string; supported: boolean; can_test: boolean; checks: { label: string; ok: boolean; help: string }[] };
const loadSetup = () => api<Setup>('/setup');
export function RuntimeSettings() {
  const resource = useResource(loadSetup);
  const mutation = useMutation();
  const [consent, setConsent] = useState(false);
  const setup = resource.data;
  return <section><h2>Local setup</h2><p>Connection and execution settings for the default local agent.</p>
    {resource.error && <p role="alert">{resource.error}</p>}
    {setup ? <><dl className="settings-facts">{[['Provider', setup.provider], ['Model', setup.model], ['Execution', setup.execution]].map(([key, value]) => <div key={key}><dt>{key}</dt><dd>{value}</dd></div>)}</dl>
      {setup.checks.map(check => <div className="settings-check" key={check.label}><strong>{check.ok ? '✓ ' : 'Needs attention: '}{check.label}</strong>{!check.ok && <p>{check.help}</p>}</div>)}
      <p>{!setup.supported ? 'Switch to the Goose runtime to use local setup.' : 'Provider availability has not been tested.'}</p></> : !resource.error && <p role="status">Checking local setup…</p>}
    <Button disabled={mutation.busy} onClick={() => { setConsent(false); void resource.refresh().catch(() => {}); }}>Refresh setup</Button>
    <p className="ui-help">Provider credentials are configured locally. Restart Cuckoo after configuration changes. A test does not boot a VM or validate agent tools.</p>
    <label className="ui-check"><input type="checkbox" checked={consent} disabled={mutation.busy} onChange={event => setConsent(event.target.checked)} />Send “Reply with OK.” to the configured provider and model. This may incur normal API charges. No conversations, files or tools are sent.</label>
    <Button disabled={!setup?.can_test || !consent || mutation.busy} onClick={() => void mutation.run(async () => {
      try { const result = await post<{ message: string }>('/setup/test', { confirm: true }); mutation.setStatus(result.message); }
      finally { setConsent(false); }
    })}>{mutation.busy ? 'Testing…' : 'Test provider connection'}</Button><p role="status">{mutation.status}</p>
  </section>;
}
export function ConnectionSettings() {
  if (workspaceId) return <WorkspaceConnections />;
  return <LocalConnections />;
}
