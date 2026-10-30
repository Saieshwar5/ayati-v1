import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import type { Connection, Delegation, Mode, Operation } from '../agents/control-model';
import { ceilings, saveApprover, type ApproverGrant, saveDelegation, type WorkspaceConnectionsData } from './workspace-connections-model';

type Props = { connection: Connection; data: WorkspaceConnectionsData; close: () => void; refresh: () => Promise<void> };
export function ConnectionAccess({ connection, data, close, refresh }: Props) {
  const members = data.people.filter(p => p.role === 'Member');
  const [selected, select] = useState(members[0]?.id ?? '');
  const mutation = useMutation();
  const person = members.find(p => p.id === selected);
  return <Dialog drawer title={`${connection.definition.name} · Configure access`} close={close} busy={mutation.busy} status={mutation.status}>
    <p className="ui-help">Choose the maximum authority this person may assign to agents they maintain. Each agent still needs its own saved permissions.</p>
    <label className="ui-field">Person<select aria-label="Person" value={selected} disabled={mutation.busy} onChange={event => select(event.target.value)}>
      <option value="">Choose a person</option>{members.map(member => <option key={member.id} value={member.id}>{member.name}</option>)}
    </select></label>
    {!members.length && <p>No workspace members yet. Admins already manage their own operation permissions.</p>}
    <p className="ui-help">You are the default approver for Ask first. This person may also select people you explicitly authorize below or in their own access settings. Changing access blocks affected agent permissions until they are reviewed again.</p>
    <h3>Agent setup limits</h3>
    {person && connection.definition.operations.map(operation => {
      const grant = data.delegations.find(d => d.connection === connection.id && d.user_id === person.id && d.operation === operation.id);
      return <DelegationRow key={`${person.id}/${operation.id}/${grant?.revision ?? 0}/${connection.revision}`}
        operation={operation} grant={grant} busy={mutation.busy}
        save={ceiling => mutation.run(async () => {
          await saveDelegation({ user: person.id, connection: connection.id, operation: operation.id, ceiling, revision: grant?.revision ?? 0, connection_revision: connection.revision });
          await refresh();
          mutation.setStatus('Access saved. Review this operation in the agent’s permissions.');
        })} />;
    })}
    <section aria-label="Business approval authority"><h3>Who may approve actions</h3><p className="ui-help">Explicitly authorize this person for individual account actions. This does not grant agent setup or source-data access. Admins can also be designated.</p>
      {person && connection.definition.operations.map(operation => {
        const grant = data.approvers?.find(g => g.user_id === person.id && g.connection === connection.id && g.operation === operation.id);
        return <ApproverRow key={`${person.id}/${operation.id}/${grant?.revision ?? 0}`} operation={operation} grant={grant} busy={mutation.busy} save={enabled => mutation.run(async () => {
          await saveApprover({ user: person.id, connection: connection.id, operation: operation.id, enabled, revision: grant?.revision ?? 0, connection_revision: connection.revision });
          await refresh(); mutation.setStatus('Approval authority saved. Review affected agent permissions.');
        })} />;
      })}
    </section>
  </Dialog>;
}

function DelegationRow({ operation, grant, busy, save }: { operation: Operation; grant?: Delegation; busy: boolean; save: (mode: Mode) => Promise<void> }) {
  const [ceiling, setCeiling] = useState<Mode>(grant?.ceiling ?? 'block');
  return <form className="connection-access-row" onSubmit={event => { event.preventDefault(); void save(ceiling); }}>
    <div><strong>{operation.title}</strong><small>{operation.effect === 'write' ? 'Changes business records' : 'Reads business records'}</small>
      {grant && grant.ceiling !== 'block' && <small>Current approver: {grant.approver_name}</small>}</div>
    <label className="ui-field">Maximum permission<select aria-label={`${operation.title} delegation`} value={ceiling} disabled={busy} onChange={event => setCeiling(event.target.value as Mode)}>
      {Object.entries(ceilings).map(([mode, label]) => <option key={mode} value={mode}>{label}</option>)}
    </select></label>
    <Button type="submit" disabled={busy || ceiling === (grant?.ceiling ?? 'block')} aria-label={`Save ${operation.title} delegation`}>Save access</Button>
  </form>;
}

function ApproverRow({ operation, grant, busy, save }: { operation: Operation; grant?: ApproverGrant; busy: boolean; save: (enabled: boolean) => Promise<void> }) {
  const [enabled, setEnabled] = useState(grant?.enabled ?? false);
  return <form className="connection-access-row" onSubmit={e => { e.preventDefault(); void save(enabled); }}>
    <label className="ui-check"><input type="checkbox" aria-label={`May approve ${operation.title}`} checked={enabled} disabled={busy} onChange={e => setEnabled(e.target.checked)} />May approve {operation.title}</label>
    <Button type="submit" disabled={busy || enabled === Boolean(grant?.enabled)} aria-label={`Save ${operation.title} approval authority`}>Save approval authority</Button>
  </form>;
}
