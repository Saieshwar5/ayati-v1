import { useState } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button, Dialog, Tabs } from './ui';
import { useMutation } from './requests';
import { MemberAccess } from './member-access';
import type { Member } from './people';

export function MemberDetails({ member, viewerRole, close, refresh }: { member: Member; viewerRole: string; close: () => void; refresh: () => Promise<void> }) {
  const [savedRole, setSavedRole] = useState(member.role);
  const [role, setRole] = useState(member.role);
  const mutation = useMutation();
  const canEdit = viewerRole === 'Owner' && savedRole !== 'Owner';
  async function change(nextRole: string | null) {
    await mutation.run(async () => {
      try { await identityRequest(`/workspaces/${workspaceId}/members/${member.id}`, { role: nextRole }); }
      catch (error) { setRole(savedRole); throw error; }
      if (nextRole === null) close();
      else { setSavedRole(nextRole); mutation.setStatus('Workspace role updated.'); }
      try { await refresh(); } catch { mutation.setStatus('Saved. Reload Users to refresh the list.'); }
    });
  }
  const membership = <section className="ui-form">
    <p className="ui-help">Workspace membership and access to business resources are managed separately.</p>
    <label className="ui-field">Workspace role<select value={role} disabled={!canEdit} onChange={event => setRole(event.target.value)}>
      {savedRole === 'Owner' ? <option>Owner</option> : <><option>Member</option><option>Admin</option></>}
    </select></label>
    <p className="ui-help">{savedRole === 'Owner' ? 'The owner manages this workspace and its resources.' : 'Admins can invite and remove members. Only the owner manages agents and shares business resources.'}</p>
    {canEdit && <Button primary onClick={() => void change(role)}>Save role</Button>}
    {savedRole !== 'Owner' && (viewerRole === 'Owner' || (viewerRole === 'Admin' && savedRole === 'Member')) &&
      <Button className="danger" onClick={() => { if (confirm(`Remove ${member.email} from this workspace? Business records remain.`)) void change(null); }}>Remove from workspace</Button>}
  </section>;
  return <Dialog title="Member details" drawer busy={mutation.busy} status={mutation.status} close={close}>
    <p className="ui-member-email">{member.email}</p>
    {canEdit ? <MemberAccess user={member.id} mutation={mutation} membership={membership} /> : <Tabs sections={[{ title: 'Membership', content: membership }]} />}
  </Dialog>;
}
