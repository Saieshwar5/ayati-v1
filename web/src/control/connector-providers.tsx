import { useState } from 'react';
import { useResource } from '../react/requests';
import { ago, codesRequest, count, money, when } from './api';
import { ReasonDialog, Tag } from './shared';

/** Each connected-app provider's health (schema 110): this month's calls,
 * errors and estimated cost, its circuit breaker and the last drift check. */
export type ConnectorHealth = {
  provider: string;
  month: { calls: number; errors: number; refused: number; usd: number };
  breaker: { state: 'closed' | 'open' | 'testing'; failures: number; until_ms: number | null; opened_ms: number | null };
  drift: { ok: number; missing: number; changed: number; new_version: number; failed: number } | null;
};
export type Connectors = {
  providers: { provider: string; enabled: boolean; note: string }[];
  configured: string[];
  health?: ConnectorHealth[];
  drift_run?: { at: number; via: 'nightly' | 'cli'; accounts: number; skipped: number } | null;
};
const loadConnectors = () => codesRequest<Connectors>('/admin/connector-providers');

function breakerText(breaker: ConnectorHealth['breaker']) {
  if (breaker.state === 'open') return `Calls paused after repeated failures, until ${when(breaker.until_ms)}. Nothing is sent meanwhile.`;
  if (breaker.state === 'testing') return 'Trying one call to see whether it recovered.';
  return breaker.failures ? `${breaker.failures} failed in a row just now` : 'No recent failures';
}

function driftText(drift: ConnectorHealth['drift'], run: Connectors['drift_run']) {
  if (!run || !drift) return 'No drift check yet.';
  const parts = [`${drift.ok} ok`, drift.changed && `${drift.changed} changed`, drift.missing && `${drift.missing} missing`,
    drift.new_version && `${drift.new_version} new version`, drift.failed && `${drift.failed} not checked`].filter(Boolean);
  return `Drift check ${ago(run.at)}: ${parts.join(' · ')}`;
}

/** Connected-app providers (Composio, operator MCP servers): health and the switch. */
export function ConnectorProviders() {
  const { data, error, setData } = useResource(loadConnectors);
  const [changing, setChanging] = useState<{ provider: string; enabled: boolean }>();
  if (!data) return error ? <p className="cc-dim cc-small">Connected-app providers: {error}</p> : null;
  const names = [...new Set([...data.configured, ...data.providers.map(p => p.provider), ...(data.health ?? []).map(h => h.provider)])];
  if (!names.length) return null;
  return <section aria-labelledby="cc-connectors">
    <h2 id="cc-connectors" className="cc-cap">Connected-app providers</h2>
    <p className="cc-dim cc-small">This month in UTC. Costs are estimates from the providers file. Check every account now with <code>ayati admin connectors verify</code>.</p>
    <div className="cc-cards">{names.map(name => {
      const state = data.providers.find(p => p.provider === name);
      const health = data.health?.find(h => h.provider === name);
      const on = state?.enabled ?? true;
      const breaker = health?.breaker.state ?? 'closed';
      const drift = health?.drift;
      const drifted = !!drift && drift.changed + drift.missing + drift.failed > 0;
      return <div key={name} className="cc-card cc-provider" aria-label={`${name} health`}>
        <span className="cc-between"><span className="cc-provider-name">{name}</span>
          <span className="cc-buttons">
            {breaker === 'open' && <Tag tone="bad">PAUSED</Tag>}
            {breaker === 'testing' && <Tag tone="warn">TESTING</Tag>}
            {drifted && <Tag tone="warn">CHANGED</Tag>}
            <Tag tone={on ? 'ok' : 'bad'}>{on ? 'ON' : 'OFF'}</Tag>
          </span></span>
        <span className="cc-dim cc-small">{on ? 'Agents can act in apps connected through it.' : `Off: “${state?.note}”`}</span>
        {health && <span className="cc-num cc-small">{count(health.month.calls)} calls · {count(health.month.errors)} errors
          {health.month.refused ? ` · ${count(health.month.refused)} refused` : ''} · {money(health.month.usd)} est.</span>}
        {health && <span className={`cc-small ${breaker === 'closed' ? 'cc-dim' : breaker === 'open' ? 'bad' : 'warn'}`}>{breakerText(health.breaker)}</span>}
        <span className={`cc-small ${drifted ? 'warn' : 'cc-dim'}`}>{driftText(drift ?? null, data.drift_run)}</span>
        <button type="button" className={`cc-btn ${on ? 'danger' : ''}`} aria-label={on ? `Turn off ${name}` : `Turn ${name} back on`}
          onClick={() => setChanging({ provider: name, enabled: !on })}>{on ? 'Turn off…' : 'Turn back on'}</button>
      </div>;
    })}</div>
    {changing && <ReasonDialog title={changing.enabled ? `Turn ${changing.provider} back on?` : `Turn off ${changing.provider} for every workspace?`}
      confirm={changing.enabled ? 'Turn back on' : 'Turn off'} danger={!changing.enabled} onClose={() => setChanging(undefined)}
      onConfirm={async reason => {
        setData(await codesRequest<Connectors>('/admin/connector-providers', { provider: changing.provider, enabled: changing.enabled, note: changing.enabled ? '' : reason, reason }));
      }}>
      <p className="cc-dim">{changing.enabled ? 'Agents can use its actions again.' : 'Its actions disappear from agents, and anything requested is refused with your reason, which people see.'}</p>
    </ReasonDialog>}
  </section>;
}
