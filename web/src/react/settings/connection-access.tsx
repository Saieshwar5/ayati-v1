import { useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import type { Connection, Delegation, Mode, Operation } from '../agents/control-model';
import { ceilings, saveDelegation, type WorkspaceConnectionsData } from './workspace-connections-model';

type Props = { connection: Connection; data: WorkspaceConnectionsData; close: () => void; refresh: () => Promise<void> };
export function ConnectionAccess({ connection, data, close, refresh }: Props) {
  const members = data.people.filter(p => p.role === 'User');
  const [selected, select] = useState(members[0]?.id ?? '');
  const mutation = useMutation();
  const person = members.find(p => p.id === selected);
  return <Dialog drawer title={`${connection.definition.name} · Configure access`} close={close} busy={mutation.busy} status={mutation.status}>
    <p className="ui-help">Choose the maximum authority this person may assign to agents they maintain. Each agent still needs its own saved permissions.</p>
    <label className="ui-field">Person<select aria-label="Person" value={selected} disabled={mutation.busy} onChange={event => select(event.target.value)}>
      <option value="">Choose a person</option>{members.map(member => <option key={member.id} value={member.id}>{member.name}</option>)}
    </select></label>
    {!members.length && <p>No workspace Users yet. Owners already manage their own operation permissions.</p>}
    <p className="ui-help">You become the approver for actions this person configures as Ask first. Changing access blocks affected agent permissions until they are reviewed again.</p>
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
