import { useState } from 'react';
import { useResource } from '../react/requests';
import { ago, control, count, type Providers, type RouteRow, type Routes } from './api';
import type { Go } from './app';
import { Loading, ReasonDialog, providerHue } from './shared';

const load = () => control<Routes>('/routes');
type Draft = { model: string; main_provider: string; main_model: string; second_provider: string; second_model: string; main_percent: number; timeout_seconds: number };

const toDraft = (route: RouteRow): Draft => ({
  model: route.model, main_provider: route.main_provider, main_model: route.main_model, second_provider: route.second_provider ?? '',
  second_model: route.second_model ?? '', main_percent: route.main_percent, timeout_seconds: route.timeout_seconds,
});
const describe = (d: Draft) => d.second_provider
  ? `${d.main_percent}% ${d.main_provider}:${d.main_model} · ${100 - d.main_percent}% ${d.second_provider}:${d.second_model} (backup) · timeout ${d.timeout_seconds} s`
  : `100% ${d.main_provider}:${d.main_model} · no backup · timeout ${d.timeout_seconds} s`;

export function RoutingScreen({ providers, selected, go }: { providers?: Providers; selected: string | null; go: Go }) {
  const { data, error, setData } = useResource(load);
  const [draft, setDraft] = useState<Draft>();
  const [dialog, setDialog] = useState<'apply' | 'remove'>();
  if (!data) return <Loading error={error} label="routes" />;
  const saved = data.routes.find(r => r.model === (draft?.model ?? selected ?? data.routes[0]?.model));
  const unrouted = data.requested.filter(m => !data.routes.some(r => r.model === m.model));
  const editing = draft ?? (saved ? toDraft(saved) : undefined);
  const changed = Boolean(draft && (!saved || JSON.stringify(draft) !== JSON.stringify(toDraft(saved))));
  const usable = providers?.providers.filter(p => p.source !== 'missing') ?? [];
  const open = (model: string) => { setDraft(undefined); go('routing', { id: model }); };
  const start = (model: string) => {
    const first = usable[0]?.name ?? '';
    setDraft({ model, main_provider: first, main_model: model, second_provider: '', second_model: '', main_percent: 100, timeout_seconds: 30 });
  };
  const edit = (change: Partial<Draft>) => editing && setDraft({ ...editing, ...change });
  const valid = editing && editing.main_provider && editing.main_model.trim() && (!editing.second_provider || editing.second_model.trim())
    && editing.timeout_seconds >= 5 && editing.timeout_seconds <= 600;
  return <div className="cc-stack">
    <p className="cc-dim cc-lead">Agents ask for a model by name. A route decides which provider answers, splits traffic between two, and sends a request to the backup when the first one fails or is too slow to start. Models without a route go to the agent’s own provider.</p>
    <div className="cc-row cc-top-align">
      <section className="cc-card cc-grow-3 cc-scroll" aria-label="Routes">
        <table className="cc-table cc-select">
          <thead><tr><th scope="col">Model agents ask for</th><th scope="col">Where traffic goes</th><th scope="col">Last hour</th></tr></thead>
          <tbody>
            {data.routes.map(r => <tr key={r.model} aria-selected={r.model === editing?.model}>
              <td><button type="button" className="cc-row-button" onClick={() => open(r.model)}><strong className="cc-mono">{r.model}</strong>
                <span className="cc-dim cc-small">changed {ago(r.changed_ms)} by {r.by}</span></button></td>
              <td><SplitBar route={toDraft(r)} providers={providers} /></td>
              <td className="cc-num cc-small">{r.traffic.length ? r.traffic.map(t => <div key={`${t.provider}:${t.model}`}>{t.provider} {count(t.calls)}{t.errors ? <span className="bad"> · {t.errors} failed</span> : ''}</div>) : <span className="cc-dim">no calls</span>}</td>
            </tr>)}
            {!data.routes.length && <tr><td colSpan={3} className="cc-empty">No routes yet: every agent uses its own provider and model.</td></tr>}
          </tbody>
        </table>
        {unrouted.length > 0 && <div className="cc-pad cc-divide-top">
          <div className="cc-cap">Models agents use without a route</div>
          <ul className="cc-plain">{unrouted.map(m => <li key={m.model} className="cc-between">
            <span><span className="cc-mono">{m.model}</span> <span className="cc-dim cc-small">{m.agents} agents · {count(m.calls_7d)} calls in 7 days</span></span>
            <button type="button" className="cc-btn ghost" onClick={() => start(m.model)}>Add route</button></li>)}</ul>
        </div>}
      </section>
      <section className="cc-card cc-grow-2" aria-labelledby="cc-route-title">
        {!editing ? <div className="cc-pad"><h2 id="cc-route-title">Choose a route</h2><p className="cc-dim">Pick a route to change it, or add one for a model agents use.</p>
          <NewRoute start={start} /></div> : <>
          <div className="cc-pad cc-divide"><h2 id="cc-route-title" className="cc-mono">{editing.model}</h2>
            <div className="cc-dim cc-small">{saved ? 'Live route' : 'New route — not live until you apply it'}</div></div>
          <div className="cc-pad cc-divide cc-stack-tight">
            <div className="cc-cap">Main</div>
            <Target providers={usable} provider={editing.main_provider} model={editing.main_model} label="Main"
              onChange={(provider, model) => edit({ main_provider: provider, main_model: model })} />
            <div className="cc-cap">Second <span className="cc-dim">(backup, optional)</span></div>
            <Target providers={usable} provider={editing.second_provider} model={editing.second_model} label="Second" optional
              onChange={(provider, model) => edit({ second_provider: provider, second_model: model, ...(provider ? {} : { main_percent: 100 }) })} />
          </div>
          <div className="cc-pad cc-divide">
            <div className="cc-between"><div className="cc-cap">Split</div><span className="cc-num cc-small">{editing.second_provider ? `${editing.main_percent}% main · ${100 - editing.main_percent}% second` : '100% main'}</span></div>
            <div className="cc-presets" role="group" aria-label="Split presets">
              {[100, 90, 70, 50].map(n => <button key={n} type="button" className="cc-chip" aria-pressed={editing.main_percent === n}
                disabled={!editing.second_provider && n !== 100} onClick={() => edit({ main_percent: n })}>{n} / {100 - n}</button>)}
            </div>
            <label className="cc-field cc-inline">Wait for a first answer<input className="cc-input cc-num cc-narrow" type="number" min={5} max={600} value={editing.timeout_seconds}
              onChange={event => edit({ timeout_seconds: Number(event.target.value) })} /> seconds, then try the other</label>
            <p className="cc-dim cc-small">The split is per request: every call in one request goes to the same model, so an answer never changes model halfway.</p>
          </div>
          <div className="cc-pad cc-buttons">
            {saved && <button type="button" className="cc-btn ghost" onClick={() => setDialog('remove')}>Remove route</button>}
          </div>
        </>}
      </section>
    </div>
    {changed && editing && <div className="cc-tray" role="region" aria-label="Changes not applied">
      <span className="cc-tag new">1 PENDING</span><span className="cc-grow-1 cc-small">{editing.model}: {saved ? `${describe(toDraft(saved))} → ` : ''}{describe(editing)} — not live yet.</span>
      <button type="button" className="cc-btn ghost" onClick={() => setDraft(undefined)}>Discard</button>
      <button type="button" className="cc-btn primary" disabled={!valid} onClick={() => setDialog('apply')}>Review and apply</button>
    </div>}
    {dialog === 'apply' && editing && <ReasonDialog title={`Apply the route for ${editing.model}`} confirm="Apply now"
      cli={`ayati admin route set ${editing.model} --main ${editing.main_provider}:${editing.main_model}${editing.second_provider ? ` --second ${editing.second_provider}:${editing.second_model} --split ${editing.main_percent}` : ''} --timeout ${editing.timeout_seconds} --reason "…"`}
      onClose={() => setDialog(undefined)} onConfirm={async reason => {
        setData(await control<Routes>('/routes', { ...editing, second_provider: editing.second_provider || null, second_model: editing.second_model || null, reason }));
        setDraft(undefined); go('routing', { id: editing.model });
      }}>
      <div className="cc-diff">{saved && <div className="cc-diff-old">− {describe(toDraft(saved))}</div>}<div className="cc-diff-new">+ {describe(editing)}</div></div>
      <p className="cc-dim cc-small">Takes effect on the next model call.</p>
    </ReasonDialog>}
    {dialog === 'remove' && saved && <ReasonDialog title={`Remove the route for ${saved.model}?`} confirm="Remove route" danger
      cli={`ayati admin route remove ${saved.model} --reason "…"`} onClose={() => setDialog(undefined)}
      onConfirm={async reason => { setData(await control<Routes>('/routes/remove', { model: saved.model, reason })); setDraft(undefined); go('routing'); }}>
      <p className="cc-dim">Agents asking for {saved.model} go to their own provider again.</p>
    </ReasonDialog>}
  </div>;
}

function SplitBar({ route, providers }: { route: Draft; providers?: Providers }) {
  const off = (name: string) => providers?.providers.some(p => p.name === name && !p.enabled) ?? false;
  const mainOff = off(route.main_provider), secondOff = !route.second_provider || off(route.second_provider);
  let main = route.second_provider ? route.main_percent : 100;
  if (mainOff) main = 0; else if (secondOff) main = 100;
  const note = mainOff && secondOff ? 'no provider available · calls fail'
    : mainOff ? `100% ${route.second_provider}:${route.second_model} · ${route.main_provider} is off`
    : main === 100 ? `100% ${route.main_provider}:${route.main_model}` : `${main}% ${route.main_provider} · ${100 - main}% ${route.second_provider}`;
  return <div className="cc-split"><div className="cc-split-bar" role="img" aria-label={note}>
    <span style={{ width: `${main}%`, background: providerHue(route.main_provider) }} />
    {route.second_provider && <span style={{ width: `${100 - main}%`, background: providerHue(route.second_provider) }} />}
  </div><span className={`cc-mono cc-small${mainOff && secondOff ? ' bad' : mainOff ? ' warn' : ''}`}>{note}</span></div>;
}

function Target({ providers, provider, model, label, optional = false, onChange }: {
  providers: Providers['providers']; provider: string; model: string; label: string; optional?: boolean; onChange: (provider: string, model: string) => void;
}) {
  const known = providers.find(p => p.name === provider)?.models.filter(m => m.enabled) ?? [];
  const list = `cc-models-${label}`;
  return <div className="cc-pair">
    <label className="cc-field">{label} provider<select className="cc-input" value={provider} onChange={event => onChange(event.target.value, event.target.value ? model : '')}>
      {optional && <option value="">None</option>}
      {providers.map(p => <option key={p.name} value={p.name}>{p.name}{p.enabled ? '' : ' (off)'}</option>)}
    </select></label>
    <label className="cc-field">{label} model<input className="cc-input cc-mono" list={list} value={model} disabled={!provider} onChange={event => onChange(provider, event.target.value)} />
      <datalist id={list}>{known.map(m => <option key={m.model} value={m.model} />)}</datalist></label>
  </div>;
}

function NewRoute({ start }: { start: (model: string) => void }) {
  const [model, setModel] = useState('');
  return <form className="cc-inline-form" onSubmit={event => { event.preventDefault(); if (model.trim()) start(model.trim()); }}>
    <label className="cc-field cc-grow-1">Model name agents ask for<input className="cc-input cc-mono" value={model} onChange={event => setModel(event.target.value)} placeholder="kimi-k2" /></label>
    <button type="submit" className="cc-btn" disabled={!model.trim()}>Start a route</button>
  </form>;
}
