import { useState } from 'react';
import { identityRequest, workspaceId, workspaceSession } from '../workspace-context';
import { Button, Dialog } from './ui';
import { useMutation } from './requests';
import { HandoverSummary, useResponsibilities } from './handover-summary';
import type { Member } from './people';

export const personName = (member: Member) => member.display_name || member.email;

/** Shows what removal moves, keeps, pauses and cancels, then removes in one request. */
export function RemovePersonDialog({ member, members, close, removed }: {
  member: Member; members: Member[]; close: () => void; removed: (message: string) => Promise<void>;
}) {
  const summary = useResponsibilities(member.id);
  const choices = members.filter(person => person.id !== member.id);
  const you = workspaceSession?.user?.id;
  const [heir, setHeir] = useState(choices.some(person => person.id === you) ? you! : choices[0]?.id ?? '');
  const mutation = useMutation();
  const name = personName(member);
  const heirName = (() => { const person = choices.find(choice => choice.id === heir); return person ? personName(person) : 'them'; })();
  const remove = () => void mutation.run(async () => {
    await identityRequest(`/workspaces/${workspaceId}/members/${member.id}`, { role: null, handover_to: heir });
    await removed(`${name} was removed. The Audit log records what moved to ${heirName}.`);
  });
  return <Dialog title={`Remove ${name}?`} busy={mutation.busy} status={mutation.status || summary.error} close={close} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button className="danger settings-remove" disabled={mutation.busy || !summary.data || !heir} onClick={remove}>{mutation.busy ? 'Removing…' : `Remove ${name}`}</Button>
  </>}>
    <p className="ui-help settings-remove-intro">{name} loses access to this workspace straight away: channels, agents and files. This is recorded in the Audit log.</p>
    <label className="ui-field">Who takes over<select value={heir} onChange={event => setHeir(event.target.value)}>
      {choices.map(person => <option key={person.id} value={person.id}>{personName(person)}{person.id === you ? ' (you)' : ''}</option>)}
    </select></label>
    <HandoverSummary summary={summary} name={name} heirName={heirName} />
  </Dialog>;
}
