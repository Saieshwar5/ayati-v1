import { useState } from 'react';
import { ago, control, type Provider, type Providers } from './api';
import type { Go } from './app';
import { ConnectorProviders } from './connector-providers';
import { Loading, ReasonDialog, Tag, providerHue } from './shared';

type Resource = { data?: Providers; error: string; setData: (data: Providers) => void; refresh: () => Promise<void> };

export function ProvidersScreen({ providers, canStoreKeys, selected, go }: { providers: Resource; canStoreKeys: boolean; selected: string | null; go: Go }) {
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
        <p className="cc-dim cc-small">The deployment’s providers file lists the providers the model catalog routes to. Their keys can live here instead of in key files.
          {!canStoreKeys && <> To keep keys here, set <code>AYATI_SECRETS_KEY_FILE</code> in the deployment.</>}</p>
        {!list.length && <p className="cc-empty">No model providers yet.</p>}
        <div className="cc-cards">{list.map(p => <ProviderCard key={p.name} provider={p} current={p.name === current?.name} open={() => go('providers', { id: p.name })} />)}</div>
      </section>
      <ConnectorProviders />
    </div>
    {current && <ProviderPanel key={current.name} provider={current} all={list} canStoreKeys={canStoreKeys} onSaved={providers.setData} go={go} />}
    {adding && <AddProvider onClose={() => setAdding(false)} onSaved={providers.setData} />}
  </div>;
}

function rate(p: Provider) {
  const tried = p.health.calls + p.health.errors;
  return tried ? 100 * p.health.errors / tried : 0;
}

function keyText(p: Provider) {
  return p.key_in_console ? `key …${p.key_hint} kept here` : p.source === 'deployment' ? 'key from its key file' : '';
}

function ProviderCard({ provider: p, current, open }: { provider: Provider; current: boolean; open: () => void }) {
  const tone = !p.enabled ? 'bad' : rate(p) >= 5 ? 'warn' : 'ok';
  return <button type="button" className="cc-card cc-provider" aria-pressed={current} onClick={open}>
    <span className="cc-between"><span className="cc-provider-name"><span className="cc-swatch" style={{ background: providerHue(p.name) }} aria-hidden="true" />{p.name}</span>
      <Tag tone={tone}>{!p.enabled ? 'OFF' : tone === 'warn' ? 'ERRORS' : 'OK'}</Tag></span>
    <span className="cc-dim cc-small">{p.source === 'console' ? 'Added in Cuckoo Control' : p.source === 'deployment' ? 'Providers file' : 'No longer configured'}
      {keyText(p) && ` · ${keyText(p)}`} · serves {p.routes.length} catalog model{p.routes.length === 1 ? '' : 's'}</span>
    <span className="cc-num cc-small">{p.health.calls} calls/h · {rate(p).toFixed(1)}% failed{p.health.p95_ms ? ` · p95 ${(p.health.p95_ms / 1000).toFixed(1)} s` : ''}</span>
  </button>;
}

type Dialog = 'key' | 'remove' | 'switch';

function ProviderPanel({ provider: p, all, canStoreKeys, onSaved, go }: { provider: Provider; all: Provider[]; canStoreKeys: boolean; onSaved: (next: Providers) => void; go: Go }) {
  const [dialog, setDialog] = useState<Dialog>();
  const [key, setKey] = useState('');
  const name = encodeURIComponent(p.name);
  const close = () => { setDialog(undefined); setKey(''); };
  // For each catalog model this provider serves: the other providers still on.
  const impact = p.routes.map(route => ({
    model: route.model,
    others: all.filter(other => other.name !== p.name && other.enabled && other.routes.some(r => r.model === route.model)).map(o => o.name),
  }));
  return <aside className="cc-card cc-grow-2" aria-labelledby="cc-provider-title">
    <div className="cc-pad cc-divide"><h2 id="cc-provider-title">{p.name}</h2>
      <div className="cc-dim cc-small">{p.source === 'console' ? 'Added in Cuckoo Control, for agents that name a provider themselves' : p.source === 'deployment' ? 'Address set in the deployment’s providers file' : 'Seen in recent calls, but no longer configured'}</div></div>
    <dl className="cc-kv cc-pad cc-divide">
      <dt>Address</dt><dd className="cc-mono">{p.base_url ?? '—'}</dd>
      <dt>API key</dt><dd>{p.key_in_console ? <><span className="cc-mono">•••• {p.key_hint}</span> <span className="cc-dim">kept here, replaced {ago(p.key_rotated_ms)}</span></>
        : p.source === 'deployment' ? 'From its key file' : '—'}</dd>
      <dt>Last hour</dt><dd className="cc-num">{p.health.calls} answered · {p.health.errors} failed{p.health.p95_ms ? ` · p95 ${(p.health.p95_ms / 1000).toFixed(1)} s` : ''}</dd>
    </dl>
    {p.source !== 'missing' && <div className="cc-pad cc-divide cc-buttons">
      <button type="button" className="cc-btn" onClick={() => setDialog('key')} disabled={!canStoreKeys}>{p.key_in_console ? 'Replace key' : 'Keep its key here'}</button>
      {(p.source === 'console' || p.key_in_console) && <button type="button" className="cc-btn ghost" onClick={() => setDialog('remove')}>
        {p.source === 'console' ? 'Remove provider' : 'Use its key file again'}</button>}
      <span className="cc-dim cc-small">Keys are sealed with the deployment’s secrets key and never shown again.</span>
    </div>}
    <div className="cc-pad cc-divide">
      <div className="cc-between"><div className="cc-cap">Catalog models it serves</div>
        <button type="button" className="cc-btn ghost" onClick={() => go('routing')}>All routes</button></div>
      {p.routes.length === 0 ? <p className="cc-dim cc-small">No catalog route names {p.name}.</p> :
        <table className="cc-table cc-compact"><thead><tr><th scope="col">Model</th><th scope="col">As</th><th scope="col">Priority</th></tr></thead>
          <tbody>{p.routes.map(r => <tr key={`${r.model}:${r.provider_model}`}><td className="cc-mono">{r.model}</td><td className="cc-mono cc-dim">{r.provider_model}</td>
            <td className="cc-num">{r.priority}</td></tr>)}</tbody></table>}
    </div>
    <div className="cc-pad cc-danger-zone">
      <div className="cc-cap bad">Emergency switch</div>
      <div className="cc-between">
        <span className="cc-small">{p.enabled ? `Stop every call to ${p.name}, for every workspace. Catalog models move to their other providers.` : `Off since ${ago(p.switch?.changed_ms)}: “${p.switch?.note}”`}</span>
        <button type="button" className={`cc-btn ${p.enabled ? 'danger' : ''}`} onClick={() => setDialog('switch')}>{p.enabled ? `Turn off ${p.name}…` : 'Turn back on'}</button>
      </div>
    </div>
    {dialog === 'switch' && <ReasonDialog title={p.enabled ? `Turn off ${p.name} for every workspace?` : `Turn ${p.name} back on?`}
      confirm={p.enabled ? `Turn off ${p.name}` : 'Turn back on'} danger={p.enabled}
      cli={`ayati admin provider ${p.enabled ? 'off' : 'on'} ${p.name} --reason "…"`} onClose={close}
      onConfirm={async reason => onSaved(await control<Providers>(`/providers/${name}/switch`, { enabled: !p.enabled, reason }))}>
      {p.enabled ? <><div className="cc-cap">What happens</div>
        {impact.length === 0 ? <p className="cc-small">No catalog model uses {p.name}. Agents that call it directly get “Cuckoo paused {p.name}” with your reason.</p> :
          <ul className="cc-impact">{impact.map(i => <li key={i.model}><span className="cc-mono">{i.model}</span>: {i.others.length
            ? <>served by <span className="cc-mono">{i.others.join(', ')}</span></> : <span className="bad">fails — no other provider is on</span>}</li>)}</ul>}
        <p className="cc-dim cc-small">Replies already streaming finish. New calls stop within seconds.</p></>
        : <p className="cc-dim">Calls start reaching {p.name} again, in the catalog’s normal order.</p>}
    </ReasonDialog>}
    {dialog === 'key' && <ReasonDialog title={p.key_in_console ? `Replace the ${p.name} API key` : `Keep the ${p.name} API key in Cuckoo Control`}
      confirm="Save key" extraValid={key.trim().length >= 8}
      cli={`ayati admin provider key ${p.name} --reason "…"  (reads the key from stdin)`} onClose={close}
      onConfirm={async reason => onSaved(await control<Providers>(`/providers/${name}/key`, { key, reason }))}>
      <label className="cc-field">API key<input className="cc-input cc-mono" type="password" autoComplete="off" value={key} onChange={event => setKey(event.target.value)} /></label>
      <p className="cc-dim cc-small">The next call uses it{p.source === 'deployment' ? ', instead of the key file' : ''}. It is never shown again.</p>
    </ReasonDialog>}
    {dialog === 'remove' && <ReasonDialog title={p.source === 'console' ? `Remove ${p.name}?` : `Use ${p.name}’s key file again?`}
      confirm={p.source === 'console' ? 'Remove provider' : 'Remove the key'} danger cli={`ayati admin provider remove ${p.name} --reason "…"`}
      onClose={close} onConfirm={async reason => onSaved(await control<Providers>(`/providers/${name}/remove`, { reason }))}>
      <p className="cc-dim">The key kept here is deleted. Past calls stay in Spend. If catalog routes need it and there is no key file, Cuckoo refuses.</p>
    </ReasonDialog>}
  </aside>;
}

function AddProvider({ onClose, onSaved }: { onClose: () => void; onSaved: (next: Providers) => void }) {
  const [form, setForm] = useState({ name: '', base_url: 'https://', key: '' });
  const valid = /^[a-z0-9][a-z0-9_-]{0,31}$/.test(form.name) && /^https:\/\/.+/.test(form.base_url) && form.key.trim().length >= 8;
  return <ReasonDialog title="Add a provider outside the providers file" confirm="Add provider" extraValid={valid}
    cli={`ayati admin provider add ${form.name || 'NAME'} --url ${form.base_url} --reason "…"  (reads the key from stdin)`}
    onClose={onClose} onConfirm={async reason => onSaved(await control<Providers>('/providers', { ...form, reason }))}>
    <label className="cc-field">Name <span className="cc-dim">(lowercase, used in routes)</span><input className="cc-input cc-mono" value={form.name} placeholder="fireworks"
      onChange={event => setForm({ ...form, name: event.target.value.toLowerCase() })} /></label>
    <label className="cc-field">OpenAI-compatible address<input className="cc-input cc-mono" value={form.base_url} placeholder="https://api.fireworks.ai/inference/v1"
      onChange={event => setForm({ ...form, base_url: event.target.value })} /></label>
    <label className="cc-field">API key<input className="cc-input cc-mono" type="password" autoComplete="off" value={form.key}
      onChange={event => setForm({ ...form, key: event.target.value })} /></label>
    <p className="cc-dim cc-small">For agents that name a provider themselves. The model catalog only routes to providers in the deployment’s
      providers file. The key is sealed before it is stored and is never shown again, here or in the CLI.</p>
  </ReasonDialog>;
}
