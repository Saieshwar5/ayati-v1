import { useState } from 'react';
import { Button } from '../ui';
import { modes } from './control-model';
import type { Draft, Preview } from './setup-model';
type Field = { type: string; enum?: (string | number | boolean)[] };
type Schema = { properties: Record<string, Field>; required: string[] };
export function SetupPreview({ draft, preview, busy, run }: { draft: Draft; preview: Preview; busy: boolean; run: (example: { connection: string; operation: string; arguments: unknown }) => void }) {
  const operations = draft.data.context.accounts.flatMap(a => a.permissions.map(p => ({ account: a, permission: p })));
  const [selected, select] = useState('0');
  const [fields, setFields] = useState<Record<string, string>>({});
  const item = operations[Number(selected)];
  const schema = item.permission.operation.schema as Schema;
  return <section aria-label="Permission preview">
    <p className="ui-help">Preview for you in the selected conversation. No business action runs. Each real request is checked again when it executes.</p>
    <div className="setup-decisions">{preview.results.map(result => {
      const p = operations.find(item => item.account.id === result.connection && item.permission.operation.id === result.operation)?.permission;
      return <p key={`${result.connection}/${result.operation}`}><strong>{p?.operation.title}</strong><span>{modes[result.decision]}{result.decision === 'ask' && ` · ${p?.approvers?.find(person => person.id === result.approver)?.name ?? p?.approver_name}`}</span></p>;
    })}</div>
    <h3>Try an example</h3>
    <label className="ui-field">Action<select aria-label="Example action" value={selected} onChange={e => { select(e.target.value); setFields({}); }}>
      {operations.map((item, i) => <option key={`${item.account.id}/${item.permission.operation.id}`} value={i}>{item.account.name} · {item.permission.operation.title}</option>)}
    </select></label>
    <form className="setup-example" onSubmit={e => { e.preventDefault(); const args = Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== '').map(([name, value]) => [name, schema.properties[name].type === 'boolean' ? value === 'true' : ['number', 'integer'].includes(schema.properties[name].type) ? Number(value) : value])); run({ connection: item.account.id, operation: item.permission.operation.id, arguments: args }); }}>
      {Object.entries(schema.properties).map(([name, field]) => <label className="ui-field" key={name}>{name.replaceAll('_', ' ')}
        {field.enum || field.type === 'boolean' ? <select aria-label={`Example ${name}`} required={schema.required.includes(name)} value={fields[name] ?? ''} onChange={e => setFields(old => ({ ...old, [name]: e.target.value }))}>
          <option value="">Choose a value</option>{(field.enum ?? [true, false]).map(value => <option key={String(value)} value={String(value)}>{String(value)}</option>)}
        </select> : <input aria-label={`Example ${name}`} type={['number', 'integer'].includes(field.type) ? 'number' : 'text'} step={field.type === 'integer' ? 1 : 'any'} maxLength={2000} required={schema.required.includes(name)} value={fields[name] ?? ''} onChange={e => setFields(old => ({ ...old, [name]: e.target.value }))} />}
      </label>)}
      <Button type="submit" disabled={busy}>Preview example</Button>
    </form>
    {preview.example && <p role="status" className="setup-example-result">Example: {modes[preview.example.decision]}{preview.example.reason && ` · ${preview.example.reason}`}</p>}
  </section>;
}
