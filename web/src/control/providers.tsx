import { useState } from 'react';
import { useResource } from '../react/requests';
import { ago, control, codesRequest, money, type CatalogModel, type Provider, type Providers, type Routes } from './api';
import type { Go } from './app';
import { Loading, ReasonDialog, Tag, providerHue } from './shared';

type Resource = { data?: Providers; error: string; setData: (data: Providers) => void; refresh: () => Promise<void> };
type Connector = { providers: { provider: string; enabled: boolean; note: string }[]; configured: string[] };
const loadRoutes = () => control<Routes>('/routes');
const loadConnectors = () => codesRequest<Connector>('/admin/connector-providers');

export function ProvidersScreen({ providers, canStoreKeys, selected, go }: { providers: Resource; canStoreKeys: boolean; selected: string | null; go: Go }) {
  const routes = useResource(loadRoutes);
  const [adding, setAdding] = useState(false);
  if (!providers.data) return <Loading error={providers.error} label="providers" />;
  const list = providers.data.providers;
  const current = list.find(p => p.name === selected) ?? list[0];
  return <div className="cc-row cc-top-align">
    <div className="cc-grow-3 cc-stack">
      <section aria-labelledby="cc-model-providers">
        <div className="cc-between"><h2 id="cc-model-providers" className="cc-cap">AI model providers</h2>
          <button type="button" className="cc-btn" onClick={() => setAdding(true)} disabled={!canStoreKeys}
            title={canStoreKeys ? undefined : 'Set AYATI_SECRETS_KEY_FILE in the deployment first'}>+ Add provider</button></div>
        {!canStoreKeys && <p className="cc-dim cc-small">To add providers and keys here, set <code>AYATI_SECRETS_KEY_FILE</code> in the deployment. Providers from the deployment’s file still show.</p>}
        {!list.length && <p className="cc-empty">No model providers yet.</p>}
        <div className="cc-cards">{list.map(p => <ProviderCard key={p.name} provider={p} current={p.name === current?.name} open={() => go('providers', { id: p.name })} />)}</div>
      </section>
      <ConnectorProviders />
    </div>
    {current && <ProviderPanel key={current.name} provider={current} routes={routes.data} canStoreKeys={canStoreKeys}
      onSaved={next => { providers.setData(next); void routes.refresh().catch(() => {}); }} />}
    {adding && <AddProvider onClose={() => setAdding(false)} onSaved={providers.setData} />}
  </div>;
}

function rate(p: Provider) {
  return p.health.calls ? 100 * p.health.errors / p.health.calls : 0;
}

function ProviderCard({ provider: p, current, open }: { provider: Provider; current: boolean; open: () => void }) {
  const tone = !p.enabled ? 'bad' : rate(p) >= 5 ? 'warn' : 'ok';
  return <button type="button" className="cc-card cc-provider" aria-pressed={current} onClick={open}>
    <span className="cc-between"><span className="cc-provider-name"><span className="cc-swatch" style={{ background: providerHue(p.name) }} aria-hidden="true" />{p.name}</span>
      <Tag tone={tone}>{!p.enabled ? 'OFF' : tone === 'warn' ? 'ERRORS' : 'OK'}</Tag></span>
    <span className="cc-dim cc-small">{p.source === 'console' ? `Added in Cuckoo Control · key …${p.key_hint}` : p.source === 'deployment' ? 'From the deployment’s providers file' : 'No longer configured'}</span>
    <span className="cc-num cc-small">{p.health.calls} calls/h · {rate(p).toFixed(1)}% errors{p.health.p95_ms ? ` · p95 ${(p.health.p95_ms / 1000).toFixed(1)} s` : ''}</span>
  </button>;
}

type Dialog = { kind: 'key' } | { kind: 'remove' } | { kind: 'switch' } | { kind: 'model'; model?: CatalogModel };

function ProviderPanel({ provider: p, routes, canStoreKeys, onSaved }: { provider: Provider; routes?: Routes; canStoreKeys: boolean; onSaved: (next: Providers) => void }) {
  const [dialog, setDialog] = useState<Dialog>();
  const [key, setKey] = useState('');
  const [model, setModel] = useState({ model: '', input: '', output: '', enabled: true });
  const name = encodeURIComponent(p.name);
  const close = () => { setDialog(undefined); setKey(''); };
  const affected = routes?.routes.filter(r => r.main_provider === p.name || r.second_provider === p.name) ?? [];
  const openModel = (existing?: CatalogModel) => {
    setModel(existing ? { model: existing.model, input: String(existing.input_per_mtok), output: String(existing.output_per_mtok), enabled: existing.enabled }
      : { model: '', input: '', output: '', enabled: true });
    setDialog({ kind: 'model', model: existing });
  };
  const prices = [Number(model.input), Number(model.output)];
  const modelValid = model.model.trim().length > 0 && !/\s/.test(model.model.trim()) && model.input !== '' && model.output !== '' && prices.every(v => Number.isFinite(v) && v >= 0);
  return <aside className="cc-card cc-grow-2" aria-labelledby="cc-provider-title">
    <div className="cc-pad cc-divide cc-between"><div><h2 id="cc-provider-title">{p.name}</h2>
      <div className="cc-dim cc-small">{p.source === 'console' ? 'Added in Cuckoo Control' : p.source === 'deployment' ? 'Set in the deployment’s providers file (read-only here)' : 'Seen in recent calls, but no longer configured'}</div></div></div>
    <dl className="cc-kv cc-pad cc-divide">
      <dt>Address</dt><dd className="cc-mono">{p.base_url ?? '—'}</dd>
      <dt>API key</dt><dd>{p.source === 'console' ? <><span className="cc-mono">•••• {p.key_hint}</span> <span className="cc-dim">replaced {ago(p.key_rotated_ms)}</span></>
        : p.source === 'deployment' ? 'In the deployment’s key file' : '—'}</dd>
      <dt>Last hour</dt><dd className="cc-num">{p.health.calls} calls · {p.health.errors} failed{p.health.p95_ms ? ` · p95 ${(p.health.p95_ms / 1000).toFixed(1)} s` : ''}</dd>
    </dl>
    {p.source === 'console' && <div className="cc-pad cc-divide cc-buttons">
      <button type="button" className="cc-btn" onClick={() => setDialog({ kind: 'key' })} disabled={!canStoreKeys}>Replace key</button>
      <button type="button" className="cc-btn ghost" onClick={() => setDialog({ kind: 'remove' })}>Remove provider</button>
      <span className="cc-dim cc-small">Keys are sealed with the deployment’s secrets key and never shown again.</span>
    </div>}
    <div className="cc-pad cc-divide">
      <div className="cc-between"><div className="cc-cap">Models and prices</div><button type="button" className="cc-btn ghost" onClick={() => openModel()}>+ Add model</button></div>
      {p.models.length === 0 ? <p className="cc-dim cc-small">No models listed. Calls to any model still work, but are not priced, so Spend shows them as unpriced.</p> :
        <table className="cc-table cc-compact"><thead><tr><th scope="col">Model</th><th scope="col">In / out per 1M</th><th scope="col">Allowed</th><th scope="col"><span className="cc-visually-hidden">Change</span></th></tr></thead>
          <tbody>{p.models.map(m => <tr key={m.model}><td className="cc-mono">{m.model}</td><td className="cc-num">{money(m.input_per_mtok)} / {money(m.output_per_mtok)}</td>
            <td>{m.enabled ? <Tag tone="ok">ALLOWED</Tag> : <Tag tone="bad">BLOCKED</Tag>}</td>
            <td><button type="button" className="cc-btn ghost" aria-label={`Change ${m.model}`} onClick={() => openModel(m)}>Change</button></td></tr>)}</tbody></table>}
    </div>
    <div className="cc-pad cc-danger-zone">
      <div className="cc-cap bad">Emergency switch</div>
      <div className="cc-between">
        <span className="cc-small">{p.enabled ? `Stop every call to ${p.name}, for every workspace. Routes move to their backups.` : `Off since ${ago(p.switch?.changed_ms)}: “${p.switch?.note}”`}</span>
        <button type="button" className={`cc-btn ${p.enabled ? 'danger' : ''}`} onClick={() => setDialog({ kind: 'switch' })}>{p.enabled ? `Turn off ${p.name}…` : 'Turn back on'}</button>
      </div>
    </div>
    {dialog?.kind === 'switch' && <ReasonDialog title={p.enabled ? `Turn off ${p.name} for every workspace?` : `Turn ${p.name} back on?`}
      confirm={p.enabled ? `Turn off ${p.name}` : 'Turn back on'} danger={p.enabled}
      cli={`ayati admin provider ${p.enabled ? 'off' : 'on'} ${p.name} --reason "…"`} onClose={close}
      onConfirm={async reason => onSaved(await control<Providers>(`/providers/${name}/switch`, { enabled: !p.enabled, reason }))}>
      {p.enabled ? <><div className="cc-cap">What happens</div>
        {affected.length === 0 ? <p className="cc-small">No route uses {p.name}. Agents that call it directly get “Cuckoo paused {p.name}” with your reason.</p> :
          <ul className="cc-impact">{affected.map(r => {
            const backup = r.main_provider === p.name ? r.second_provider : r.main_provider;
            return <li key={r.model}><span className="cc-mono">{r.model}</span>: {backup ? <>all traffic to <span className="cc-mono">{backup}</span></> : <span className="bad">fails — no backup</span>}</li>;
          })}</ul>}
        <p className="cc-dim cc-small">Replies already streaming finish. New calls stop within seconds.</p></>
        : <p className="cc-dim">Calls start reaching {p.name} again; routes use their normal split.</p>}
    </ReasonDialog>}
    {dialog?.kind === 'key' && <ReasonDialog title={`Replace the ${p.name} API key`} confirm="Replace key" extraValid={key.trim().length >= 8}
      cli={`ayati admin provider rotate-key ${p.name} --reason "…"  (reads the key from stdin)`} onClose={close}
      onConfirm={async reason => onSaved(await control<Providers>(`/providers/${name}/key`, { key, reason }))}>
      <label className="cc-field">New API key<input className="cc-input cc-mono" type="password" autoComplete="off" value={key} onChange={event => setKey(event.target.value)} /></label>
      <p className="cc-dim cc-small">The next call uses the new key. The old one is gone from Cuckoo once you save.</p>
    </ReasonDialog>}
    {dialog?.kind === 'remove' && <ReasonDialog title={`Remove ${p.name}?`} confirm="Remove provider" danger cli={`ayati admin provider remove ${p.name} --reason "…"`}
      onClose={close} onConfirm={async reason => onSaved(await control<Providers>(`/providers/${name}/remove`, { reason }))}>
      <p className="cc-dim">Its key is deleted. Its past calls stay in Spend. Routes that use it must change first.</p>
    </ReasonDialog>}
    {dialog?.kind === 'model' && <ReasonDialog title={dialog.model ? `Change ${dialog.model.model}` : `Add a ${p.name} model`} confirm="Save model" extraValid={modelValid}
      cli={`ayati admin model set ${p.name} ${model.model || 'MODEL'} --input ${model.input || '…'} --output ${model.output || '…'}${model.enabled ? '' : ' --blocked'} --reason "…"`}
      onClose={close} onConfirm={async reason => onSaved(await control<Providers>('/models', {
        provider: p.name, model: model.model.trim(), enabled: model.enabled, input_per_mtok: prices[0], output_per_mtok: prices[1], reason,
      }))}>
      <label className="cc-field">Model id, exactly as {p.name} names it<input className="cc-input cc-mono" value={model.model} readOnly={Boolean(dialog.model)}
        onChange={event => setModel({ ...model, model: event.target.value })} /></label>
      <div className="cc-pair">
        <label className="cc-field">Input $ per 1M tokens<input className="cc-input cc-num" inputMode="decimal" value={model.input} onChange={event => setModel({ ...model, input: event.target.value })} /></label>
        <label className="cc-field">Output $ per 1M tokens<input className="cc-input cc-num" inputMode="decimal" value={model.output} onChange={event => setModel({ ...model, output: event.target.value })} /></label>
      </div>
      <label className="cc-check"><input type="checkbox" checked={model.enabled} onChange={event => setModel({ ...model, enabled: event.target.checked })} /> Allowed: routes and agents may use it</label>
      <p className="cc-dim cc-small">Prices apply to calls from now on; earlier calls keep the price they had.</p>
    </ReasonDialog>}
  </aside>;
}

function AddProvider({ onClose, onSaved }: { onClose: () => void; onSaved: (next: Providers) => void }) {
  const [form, setForm] = useState({ name: '', base_url: 'https://', key: '' });
  const valid = /^[a-z0-9][a-z0-9_-]{0,31}$/.test(form.name) && /^https:\/\/.+/.test(form.base_url) && form.key.trim().length >= 8;
  return <ReasonDialog title="Add a model provider" confirm="Add provider" extraValid={valid}
    cli={`ayati admin provider add ${form.name || 'NAME'} --url ${form.base_url} --reason "…"  (reads the key from stdin)`}
    onClose={onClose} onConfirm={async reason => onSaved(await control<Providers>('/providers', { ...form, reason }))}>
    <label className="cc-field">Name <span className="cc-dim">(lowercase, used in routes)</span><input className="cc-input cc-mono" value={form.name} placeholder="fireworks"
      onChange={event => setForm({ ...form, name: event.target.value.toLowerCase() })} /></label>
    <label className="cc-field">OpenAI-compatible address<input className="cc-input cc-mono" value={form.base_url} placeholder="https://api.fireworks.ai/inference/v1"
      onChange={event => setForm({ ...form, base_url: event.target.value })} /></label>
    <label className="cc-field">API key<input className="cc-input cc-mono" type="password" autoComplete="off" value={form.key}
      onChange={event => setForm({ ...form, key: event.target.value })} /></label>
    <p className="cc-dim cc-small">The key is sealed before it is stored and is never shown again, here or in the CLI.</p>
  </ReasonDialog>;
}

/** Connected-app providers (Composio): the existing per-provider switch. */
function ConnectorProviders() {
  const { data, error, setData } = useResource(loadConnectors);
  const [changing, setChanging] = useState<{ provider: string; enabled: boolean }>();
  if (!data) return error ? <p className="cc-dim cc-small">Connected-app providers: {error}</p> : null;
  const names = [...new Set([...data.configured, ...data.providers.map(p => p.provider)])];
  if (!names.length) return null;
  return <section aria-labelledby="cc-connectors">
    <h2 id="cc-connectors" className="cc-cap">Connected-app providers</h2>
    <div className="cc-cards">{names.map(name => {
      const state = data.providers.find(p => p.provider === name);
      const on = state?.enabled ?? true;
      return <div key={name} className="cc-card cc-provider">
        <span className="cc-between"><span className="cc-provider-name">{name}</span><Tag tone={on ? 'ok' : 'bad'}>{on ? 'ON' : 'OFF'}</Tag></span>
        <span className="cc-dim cc-small">{on ? 'Agents can act in apps connected through it.' : `Off: “${state?.note}”`}</span>
        <button type="button" className={`cc-btn ${on ? 'danger' : ''}`} onClick={() => setChanging({ provider: name, enabled: !on })}>{on ? 'Turn off…' : 'Turn back on'}</button>
      </div>;
    })}</div>
    {changing && <ReasonDialog title={changing.enabled ? `Turn ${changing.provider} back on?` : `Turn off ${changing.provider} for every workspace?`}
      confirm={changing.enabled ? 'Turn back on' : 'Turn off'} danger={!changing.enabled} onClose={() => setChanging(undefined)}
      onConfirm={async reason => {
        setData(await codesRequest<Connector>('/admin/connector-providers', { provider: changing.provider, enabled: changing.enabled, note: changing.enabled ? '' : reason, reason }));
      }}>
      <p className="cc-dim">{changing.enabled ? 'Agents can use its actions again.' : 'Its actions disappear from agents, and anything requested is refused with your reason, which people see.'}</p>
    </ReasonDialog>}
  </section>;
}
