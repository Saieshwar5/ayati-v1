import { modes, type Mode } from './control-model';
import type { Choice, Draft } from './setup-model';

export function SetupReview({ draft, choices, change, executionAvailable }: { draft: Draft; choices: Choice[]; change: (index: number, mode: Mode) => void; executionAvailable: boolean }) {
  return <section aria-label="Review action permissions">
    <p className="ui-help">Draft only. Review each action before activation. Amount, time and individual-record conditions are not supported.</p>
    {draft.data.notes.length > 0 && <details className="setup-notes"><summary>Suggestion and review notes</summary><ul>{draft.data.notes.map((note, i) => <li key={i}>{note}</li>)}</ul></details>}
    {draft.data.context.accounts.map(account => <section className="setup-account" key={account.id} aria-label={account.name}>
      <h3>{account.name}</h3><p className="ui-help">{account.account || 'Configured business account'}</p>
      {account.permissions.map(permission => {
        const index = choices.findIndex(c => c.connection === account.id && c.operation === permission.operation.id);
        return <div className="setup-action" key={permission.operation.id}>
          <div><strong>{permission.operation.title}</strong><p>{permission.operation.effect === 'read' ? 'Reads business records' : 'Changes business records'}</p>
            <p>Your limit: {modes[permission.ceiling]}{permission.approver_name && ` · Ask requests go to ${permission.approver_name}`}</p></div>
          <label className="ui-field">Permission<select aria-label={`${account.name}: ${permission.operation.title} permission`} value={choices[index].mode} disabled={permission.ceiling === 'block'} onChange={e => change(index, e.target.value as Mode)}>
            {Object.entries(modes).map(([mode, label]) => <option key={mode} value={mode} disabled={(mode !== 'block' && !executionAvailable) || (permission.ceiling === 'ask' && mode === 'allow')}>{label}</option>)}
          </select></label>
          {permission.operation.tool && <details><summary>Connected action details</summary><p>{permission.operation.id} · {permission.operation.tool}</p></details>}
        </div>;
      })}
    </section>)}
    <Coverage draft={draft} />
  </section>;
}
export function Coverage({ draft }: { draft: Draft }) {
  const unsupported = draft.data.inventories.flatMap(i => i.unsupported.map(tool => ({ connection: i.connection, tool })));
  const total = draft.data.inventories.reduce((n, i) => n + i.tools.length, 0);
  return <section className="setup-coverage" aria-label="Tool coverage"><strong>{draft.data.choices.length} reviewed {draft.data.choices.length === 1 ? 'action' : 'actions'} · {unsupported.length} unsupported {unsupported.length === 1 ? 'tool' : 'tools'} · {total} discovered {total === 1 ? 'tool' : 'tools'}</strong>
    <p className="ui-help">Every discovered tool is accounted for. Unsupported and newly added tools receive no permission. Tool descriptions alone do not prove safe behavior.</p>
    {unsupported.length > 0 && <details><summary>Unavailable tools</summary><ul>{unsupported.map(item => <li key={`${item.connection}/${item.tool}`}>{item.tool} · requires operator review</li>)}</ul></details>}
  </section>;
}
