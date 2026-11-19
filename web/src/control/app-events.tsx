import { useResource, useMutation } from '../react/requests';
import { codesRequest, when } from './api';
import { Tag } from './shared';

/** Whether the connected-app provider sends app events (a new email, a new
 * calendar event) to this deployment. Setting it up points the provider's one
 * project webhook here; the server keeps the signing secret sealed. */
type AppEventSetup = {
  set_up: boolean;
  setup: { webhook_url: string; changed: number; changed_by: string | null } | null;
  webhook_url: string | null;
  public_origin: boolean;
  secrets_key: boolean;
  configured: boolean;
};
const path = '/admin/connector-events/composio';
const load = () => codesRequest<AppEventSetup>(path);

function missing(data: AppEventSetup) {
  if (!data.configured) return 'Add Composio to the deployment’s connector providers first.';
  if (!data.public_origin) return 'Set AYATI_PUBLIC_ORIGIN to this server’s public HTTPS address first.';
  if (!data.secrets_key) return 'Set AYATI_SECRETS_KEY_FILE in the deployment first: the signing secret is kept sealed.';
  return '';
}

export function AppEvents() {
  const { data, error, setData } = useResource(load);
  const m = useMutation();
  if (!data) return error ? <p className="cc-dim cc-small">App events: {error}</p> : null;
  const blocked = missing(data);
  // The address moved (a new hostname): events still go to the old one until set up again.
  const moved = data.set_up && !!data.webhook_url && data.setup?.webhook_url !== data.webhook_url;
  return <section aria-labelledby="cc-app-events">
    <h2 id="cc-app-events" className="cc-cap">App events</h2>
    <p className="cc-dim cc-small">Lets a new email or calendar event start an agent’s automation. Composio sends every workspace’s events to one address on this server.</p>
    <div className="cc-card cc-provider" aria-label="App events from Composio">
      <span className="cc-between"><span className="cc-provider-name">composio</span>
        <Tag tone={data.set_up && !moved ? 'ok' : 'warn'}>{data.set_up ? moved ? 'MOVED' : 'SET UP' : 'NOT SET UP'}</Tag></span>
      {data.setup && <span className="cc-dim cc-small">Events go to {data.setup.webhook_url}. Set up {when(data.setup.changed)}{data.setup.changed_by ? ` by ${data.setup.changed_by}` : ''}.</span>}
      {moved && <span className="cc-small warn">This server is now {data.webhook_url}. Set up again to move events here.</span>}
      {!data.set_up && !blocked && <span className="cc-dim cc-small">Events will go to {data.webhook_url}.</span>}
      {blocked && <span className="cc-small warn">{blocked}</span>}
      {m.status && <span className="cc-small bad" role="alert">{m.status}</span>}
      <button type="button" className="cc-btn" disabled={m.busy || !!blocked}
        onClick={() => void m.run(async () => { setData(await codesRequest<AppEventSetup>(path, {})); })}>
        {m.busy ? 'Setting up…' : data.set_up ? 'Set up again' : 'Set up app events'}</button>
    </div>
  </section>;
}
