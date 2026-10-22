import { useState } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button, Dialog, Tabs } from './ui';
import { useMutation } from './requests';
import { MemberAccess } from './member-access';
import type { Member } from './people';

export function MemberDetails({ member, viewerRole, ownerCount, close, refresh }: { member: Member; viewerRole: string; ownerCount: number; close: () => void; refresh: () => Promise<void> }) {
  const [savedRole, setSavedRole] = useState(member.role);
  const [role, setRole] = useState(member.role);
  const mutation = useMutation();
  const canEdit = viewerRole === 'Owner';
  const lastOwner = savedRole === 'Owner' && ownerCount < 2;
  async function change(nextRole: string | null) {
    if (nextRole === 'Owner' && savedRole !== 'Owner' && !confirm('Make this person an Owner? They can manage all workspace settings, invite and remove people, and change ownership roles.')) return;
    await mutation.run(async () => {
      try { await identityRequest(`/workspaces/${workspaceId}/members/${member.id}`, { role: nextRole }); }
      catch (error) { setRole(savedRole); throw error; }
      if (nextRole === null) close();
      else { setSavedRole(nextRole); mutation.setStatus('Workspace role updated.'); }
      try { await refresh(); } catch { mutation.setStatus('Saved. Reload People to refresh the list.'); }
    });
  }
  const membership = <section className="ui-form">
    <p className="ui-help">Workspace membership and access to business resources are managed separately.</p>
    <label className="ui-field">Workspace role<select aria-label="Workspace role" value={role} disabled={!canEdit || lastOwner} onChange={event => setRole(event.target.value)}>
      <option>User</option><option>Owner</option>
    </select></label>
    <p className="ui-help">{lastOwner ? 'This workspace must keep at least one Owner. Make another User an Owner first.' : 'Owners have equal control of this workspace. Users use the resources shared with them.'}</p>
    {canEdit && !lastOwner && <Button primary onClick={() => void change(role)}>Save role</Button>}
    {canEdit && !lastOwner && <Button className="danger" onClick={() => { if (confirm(`Remove ${member.email} from this workspace? Access ends; shared records remain.`)) void change(null); }}>Remove from workspace</Button>}
  </section>;
  return <Dialog title="Person details" drawer busy={mutation.busy} status={mutation.status} close={close}>
    {member.display_name && <h3>{member.display_name}</h3>}
    <p className="ui-member-email">{member.email}</p>
    {canEdit ? <MemberAccess user={member.id} mutation={mutation} membership={membership} /> : <Tabs sections={[{ title: 'Membership', content: membership }]} />}
  </Dialog>;
}
