import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { comparisons, loadRules, numeric, readBack, saveRule, type CompanyRule, type Comparator, type Effect, type RuleAction, type RuleInput, type RuleKind } from './company-rules-model';

/** Settings → Company rules (core-agent CA1): conditions every agent must follow on
 * top of each action's Allow / Ask first / Block. Everyone reads them; admins edit. */
export function CompanyRules() {
  const resource = useResource(loadRules);
  const toggle = useMutation();
  const [editing, setEditing] = useState<CompanyRule | 'new'>();
  const data = resource.data;
  const flip = (rule: CompanyRule) => void toggle.run(async () => {
    await saveRule({ ...rule, enabled: !rule.enabled });
    await resource.refresh();
    toggle.setStatus(rule.enabled ? 'Rule turned off.' : 'Rule turned on.');
  });
  return <section className="company-rules" aria-label="Company rules">
    <header className="settings-section-heading"><div><h2>Company rules</h2>
      <p>Conditions every agent must follow, on top of each action's Allow, Ask first or Block. A rule can only make an agent stricter.</p></div>
      {data?.can_manage && <Button primary disabled={!data.actions.length} onClick={() => setEditing('new')}>Add rule</Button>}</header>
    {resource.error && <><p role="alert">{resource.error}</p><Button onClick={() => void resource.refresh().catch(() => {})}>Retry</Button></>}
    {!data && !resource.error && <p role="status">Loading company rules…</p>}
    {data && !data.rules.length && <p className="ui-empty">{data.actions.length ? 'No company rules yet.' : 'Connect an account in Connections first; rules apply to its actions.'}</p>}
    {data && data.rules.length > 0 && <ul className="settings-rows">{data.rules.map(rule => <li key={rule.id} className="settings-row">
      <span><strong>{rule.summary}</strong>
        <small>{[rule.enabled ? '' : 'Off', rule.note, rule.updated_by_name ? `Changed by ${rule.updated_by_name}` : ''].filter(Boolean).join(' · ')}</small></span>
      {data.can_manage && <span className="company-rule-actions">
        <Button disabled={toggle.busy} onClick={() => setEditing(rule)}>Edit</Button>
        <Button disabled={toggle.busy} onClick={() => flip(rule)}>{rule.enabled ? 'Turn off' : 'Turn on'}</Button></span>}
    </li>)}</ul>}
    <p className="ui-status" role="status">{toggle.status}</p>
    {editing && data && <RuleDialog actions={data.actions} rule={editing === 'new' ? undefined : editing}
      close={() => setEditing(undefined)} saved={async () => { setEditing(undefined); await resource.refresh(); toggle.setStatus('Company rule saved.'); }} />}
  </section>;
}

const key = (action: Pick<RuleAction, 'connection' | 'operation'>) => `${action.connection}|${action.operation}`;

function RuleDialog({ actions, rule, close, saved }: { actions: RuleAction[]; rule?: CompanyRule; close: () => void; saved: () => Promise<void> }) {
  const first = actions.find(a => a.operation === rule?.operation && (!rule?.connection || a.connection === rule.connection)) ?? actions[0];
  const [actionKey, setActionKey] = useState(key(first));
  const [kind, setKind] = useState<RuleKind>(rule?.kind ?? 'field');
  const action = actions.find(a => key(a) === actionKey) ?? first;
  const choices = action.fields.filter(f => kind === 'field' || numeric(f));
  const [fieldName, setFieldName] = useState(rule?.field ?? choices.find(numeric)?.name ?? choices[0]?.name ?? '');
  const field = choices.find(f => f.name === fieldName) ?? choices[0];
  const allowed: Comparator[] = kind === 'daily_total' ? ['>', '>='] : numeric(field) ? ['>', '>=', '<', '<=', '=', '!='] : ['=', '!='];
  const [chosen, setComparator] = useState<Comparator>(rule?.comparator ?? '>');
  const comparator = allowed.includes(chosen) ? chosen : allowed[0];
  const [text, setText] = useState(rule === undefined ? '' : String(rule.value));
  const [effect, setEffect] = useState<Effect>(rule?.effect ?? 'ask');
  const [note, setNote] = useState(rule?.note ?? '');
  const [id] = useState(() => rule?.id ?? crypto.randomUUID());
  const mutation = useMutation();
  const value = !field ? '' : numeric(field) ? (text.trim() === '' ? NaN : Number(text)) : field.type === 'boolean' ? text === 'true' : text;
  const ready = Boolean(field) && (typeof value !== 'number' || Number.isFinite(value)) && (field?.type !== 'string' || text.trim() !== '');
  const input: RuleInput | undefined = field && ready ? { id, operation: action.operation, connection: action.connection, kind, field: field.name, comparator, value, effect, note: note.trim(), enabled: rule?.enabled ?? true, revision: rule?.revision ?? 0 } : undefined;
  const submit = () => input && void mutation.run(async () => { await saveRule(input); await saved(); });
  return <Dialog title={rule ? 'Edit company rule' : 'Add a company rule'} close={close} busy={mutation.busy} status={mutation.status}
    footer={<><Button onClick={close}>Cancel</Button><Button primary disabled={!input || mutation.busy} onClick={submit}>Save rule</Button></>}>
    <div className="ui-form company-rule-form">
      <label>Action<select value={actionKey} onChange={event => setActionKey(event.target.value)}>
        {actions.map(a => <option key={key(a)} value={key(a)}>{a.account} · {a.title}</option>)}</select></label>
      <fieldset className="company-rule-kind"><legend>Check</legend>
        <label><input type="radio" name="kind" checked={kind === 'field'} onChange={() => setKind('field')} />Each request</label>
        <label><input type="radio" name="kind" checked={kind === 'daily_total'} onChange={() => setKind('daily_total')} />Today's total</label>
      </fieldset>
      {!choices.length && <p role="alert">This action has no {kind === 'daily_total' ? 'amount' : ''} fields a rule can check.</p>}
      {field && <>
        <label>Field<select value={field.name} onChange={event => setFieldName(event.target.value)}>
          {choices.map(f => <option key={f.name} value={f.name}>{f.name.replaceAll('_', ' ')}</option>)}</select></label>
        <label>Comparison<select value={comparator} onChange={event => setComparator(event.target.value as Comparator)}>
          {allowed.map(c => <option key={c} value={c}>{kind === 'daily_total' ? (c === '>=' ? 'would reach' : 'would go over') : comparisons[c]}</option>)}</select></label>
        {field.type === 'boolean'
          ? <label>Value<select value={text === 'true' ? 'true' : 'false'} onChange={event => setText(event.target.value)}><option value="true">Yes</option><option value="false">No</option></select></label>
          : <label>Value<input type={numeric(field) ? 'number' : 'text'} value={text} onChange={event => setText(event.target.value)} /></label>}
      </>}
      <fieldset className="company-rule-kind"><legend>Then</legend>
        <label><input type="radio" name="effect" checked={effect === 'ask'} onChange={() => setEffect('ask')} />Ask first</label>
        <label><input type="radio" name="effect" checked={effect === 'block'} onChange={() => setEffect('block')} />Block</label>
      </fieldset>
      <label>Note <small>optional</small><input value={note} maxLength={280} onChange={event => setNote(event.target.value)} /></label>
      <p className="company-rule-readback" aria-live="polite">{input ? readBack(input, action.title) : 'Choose a field and a value to see the rule in plain words.'}</p>
      <p className="ui-help">If a request doesn't include the field, a person decides. Waiting approvals count towards today's total.</p>
    </div>
  </Dialog>;
}
