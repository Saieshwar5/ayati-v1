import { useState } from 'react';
import { useResource } from '../react/requests';
import { control, money, when, type ConsoleSetting, type Settings } from './api';
import { Loading, ReasonDialog, Tag } from './shared';

const load = () => control<Settings>('/settings');
const shown = (s: Pick<ConsoleSetting, 'kind' | 'value'>) => s.kind === 'bool' ? (s.value ? 'On' : 'Off') : s.value === null ? 'No limit' : money(Number(s.value));

export function ConfigurationScreen() {
  const { data, error, setData } = useResource(load);
  const [editing, setEditing] = useState<ConsoleSetting>();
  const [value, setValue] = useState('');
  if (!data) return <Loading error={error} label="configuration" />;
  const open = (setting: ConsoleSetting) => {
    setEditing(setting);
    setValue(setting.kind === 'bool' ? String(!setting.value) : setting.value === null ? '' : String(setting.value));
  };
  const amount = value.trim().replace('$', '');
  const valid = editing?.kind === 'bool' || amount === '' || (Number.isFinite(Number(amount)) && Number(amount) >= 0);
  return <div className="cc-stack cc-narrow-page">
    <section aria-labelledby="cc-console-settings">
      <h2 id="cc-console-settings" className="cc-cap">Set here</h2>
      <div className="cc-card">{data.settings.map(setting => <div key={setting.key} className="cc-setting">
        <div className="cc-grow-1"><div className="cc-mono">{setting.key}</div><div className="cc-dim cc-small">{setting.about}</div>
          {setting.changed_ms && <div className="cc-dim cc-small">Changed {when(setting.changed_ms)} by {setting.by}</div>}
          {setting.history.length > 1 && <details className="cc-small"><summary>History</summary><ul className="cc-history">{setting.history.map(h =>
            <li key={`${h.at_ms}-${h.summary}`}><span className="cc-dim cc-num">{when(h.at_ms)}</span> {h.by} {h.summary} <span className="cc-dim">— “{h.reason}”</span></li>)}</ul></details>}
        </div>
        <Tag tone="new">CONSOLE</Tag>
        <span className="cc-num cc-setting-value">{shown(setting)}</span>
        <button type="button" className="cc-btn" onClick={() => open(setting)}>{setting.kind === 'bool' ? (setting.value ? 'Turn off' : 'Turn on') : 'Change'}</button>
      </div>)}</div>
    </section>
    <section aria-labelledby="cc-deployment-settings">
      <h2 id="cc-deployment-settings" className="cc-cap">Set by the deployment</h2>
      <p className="cc-dim cc-small">Read-only on purpose: change these in the deployment’s environment and restart. Someone who gets into the console still cannot change who the operators are.</p>
      <div className="cc-card cc-scroll"><table className="cc-table">
        <thead><tr><th scope="col">Setting</th><th scope="col">Value</th><th scope="col">Variable</th></tr></thead>
        <tbody>{data.deployment.map(s => <tr key={s.key}><td><div>{s.about}</div></td><td className="cc-mono">{s.value === null ? 'not set' : String(s.value)}</td>
          <td><Tag tone="off">ENV</Tag> <span className="cc-mono cc-small">{s.env}</span></td></tr>)}</tbody>
      </table></div>
    </section>
    {editing && <ReasonDialog title={`Change ${editing.key}`} confirm="Save setting" extraValid={valid}
      cli={`ayati admin setting set ${editing.key} ${editing.kind === 'bool' ? value : amount || 'none'} --reason "…"`}
      onClose={() => setEditing(undefined)} onConfirm={async reason => {
        const next = editing.kind === 'bool' ? value === 'true' : amount === '' ? null : Number(amount);
        setData(await control<Settings>('/settings', { key: editing.key, value: next, reason }));
      }}>
      {editing.kind === 'bool' ? <p>{shown(editing)} → <strong>{value === 'true' ? 'On' : 'Off'}</strong></p>
        : <label className="cc-field">Dollars <span className="cc-dim">(empty: no limit)</span>
          <input className="cc-input cc-num" inputMode="decimal" value={value} onChange={event => setValue(event.target.value)} placeholder="no limit" /></label>}
      <p className="cc-dim cc-small">{editing.about}</p>
    </ReasonDialog>}
  </div>;
}
