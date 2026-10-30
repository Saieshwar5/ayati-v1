import { useState } from 'react';
import { currentWorkspace, identityRequest, workspaceId, workspaceSession } from '../../workspace-context';
import type { Participant } from '../chat/model';
import { HandoverSummary, useResponsibilities } from '../handover-summary';
import { useMutation } from '../requests';
import { Button, Dialog } from '../ui';

/** A person leaves on their own, choosing who takes over their shared agents and Library
 * items (an admin by default). The server refuses the last admin. */
export function LeaveWorkspaceDialog({ people, close }: { people: Participant[]; close: () => void }) {
  const you = workspaceSession?.user?.id ?? '';
  const choices = people.filter(person => person.id !== you);
  const [heir, setHeir] = useState(choices.find(person => person.role === 'Admin')?.id ?? choices[0]?.id ?? '');
  const summary = useResponsibilities(you);
  const mutation = useMutation();
  // Stays locked after success until the page unloads, so a second click sends nothing.
  const [left, setLeft] = useState(false);
  const heirName = choices.find(person => person.id === heir)?.name ?? 'them';
  const leave = () => void mutation.run(async () => {
    await identityRequest(`/workspaces/${workspaceId}/members/${you}`, { role: null, handover_to: heir });
    setLeft(true);
    // A full page load, so nothing from this workspace stays open after access ends.
    location.assign('/?view=account&choose=1');
  });
  return <Dialog title={`Leave ${currentWorkspace()?.name ?? 'this workspace'}?`} busy={mutation.busy || left} status={mutation.status || summary.error} close={close} footer={<>
    <Button disabled={mutation.busy || left} onClick={close}>Cancel</Button>
    <Button className="danger settings-remove" disabled={mutation.busy || left || !summary.data || !heir} onClick={leave}>{mutation.busy || left ? 'Leaving…' : 'Leave workspace'}</Button>
  </>}>
    <p className="ui-help settings-remove-intro">You lose access straight away: channels, agents and files. To come back, an admin has to invite you again.</p>
    <label className="ui-field">Who takes over<select value={heir} onChange={event => setHeir(event.target.value)}>
      {choices.map(person => <option key={person.id} value={person.id}>{person.name}{person.role === 'Admin' ? ' (admin)' : ''}</option>)}
    </select></label>
    <HandoverSummary summary={summary} name="you" heirName={heirName} self />
  </Dialog>;
}
