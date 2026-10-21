import { useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button, Dialog, Tabs } from './ui';
import { useMutation, useResource } from './requests';
import { MemberDetails } from './member-details';

export type Member = { id: string; email: string; display_name?: string | null; role: string };
type People = { role: string; members: Member[]; invitations: { id: string; email: string; role: string; status: string; expires: number }[] };
const base = `/workspaces/${workspaceId}`;
const loadPeople = () => identityRequest<People>(`${base}/people`);

export function PeoplePage() {
  const { data, error, refresh } = useResource(loadPeople);
  const [query, setQuery] = useState('');
  const [inviting, setInviting] = useState(false);
  const [selected, select] = useState<Member>();
  const mutation = useMutation();
  const matches = (email: string) => email.toLowerCase().includes(query.trim().toLowerCase());
  async function saved(message: string) {
    mutation.setStatus(message);
    try { await refresh(); } catch { mutation.setStatus(`${message} Reload Users to refresh the list.`); }
  }
  const members = data?.members.filter(member => matches(member.email) || matches(member.display_name ?? '')) ?? [];
  const invitations = data?.invitations.filter(invitation => matches(invitation.email)) ?? [];
  return <div id="app"><main className="desk-page" aria-label="Users">
    <header className="desk-toolbar"><div className="desk-title"><h1>Users</h1><p>Your team and their workspace access.</p></div>
      <input type="search" className="ui-search" placeholder="Find a person" aria-label="Find a person" value={query} onChange={event => setQuery(event.target.value)} />
      <Button primary hidden={!data || data.role === 'Member'} onClick={() => setInviting(true)}>＋ Invite person</Button>
    </header><p className="ui-status" role="status">{mutation.status || error || (!data ? 'Loading your team…' : '')}</p>
    {error && <Button onClick={() => void refresh().catch(() => {})}>Retry</Button>}
    <div className="desk-content"><Tabs sections={[
      { title: 'Members', content: <div className="ui-directory">{members.map(member => <div className="ui-row" key={member.id}>
        <span className="ui-avatar" aria-hidden="true">{(member.display_name || member.email).slice(0, 2).toUpperCase()}</span>
        <div className="ui-row-title"><strong>{member.display_name || member.email}</strong>{member.display_name && <small>{member.email}</small>}</div><span className="ui-badge">{member.role}</span>
        <Button aria-label={`View ${member.email}`} data-member={member.id} onClick={() => select(member)}>Details</Button>
      </div>)}{data && !members.length && <p className="ui-empty">{query ? 'No members match your search.' : 'No members to show.'}</p>}</div> },
      { title: 'Invitations', content: <div className="ui-directory">{invitations.map(invitation => <div className="ui-row" key={invitation.id}>
        <div className="ui-row-title"><strong>{invitation.email}</strong><small>{invitation.role}</small></div>
        <span className="ui-badge">{invitation.expires * 1000 <= Date.now() ? 'Expired' : invitation.status}</span>
        {data?.role !== 'Member' && <Button disabled={mutation.busy} onClick={() => {
          if (!confirm(`Cancel the invitation for ${invitation.email}?`)) return;
          void mutation.run(async () => {
            const result = await identityRequest<{ provider_invitation_revoked: boolean }>(`${base}/invitations/${invitation.id}/cancel`, {});
            await saved(result.provider_invitation_revoked ? 'Invitation cancelled.' : 'Workspace invitation cancelled. The email link may still allow account signup, but cannot grant workspace access.');
          });
        }}>Cancel invitation</Button>}
      </div>)}{data && !invitations.length && <p className="ui-empty">{query ? 'No invitations match your search.' : 'No invitations to show. Invite someone when you’re ready.'}</p>}</div> },
    ]} /></div>
    {inviting && data && <InviteDialog viewerRole={data.role} close={() => setInviting(false)} saved={async () => { setInviting(false); await saved('Invitation sent. They can sign in and accept it under Your workspaces.'); }} />}
    {selected && data && <MemberDetails key={selected.id} member={selected} viewerRole={data.role} close={() => select(undefined)} refresh={refresh} />}
  </main></div>;
}

function InviteDialog({ viewerRole, close, saved }: { viewerRole: string; close: () => void; saved: () => Promise<void> }) {
  const emailInput = useRef<HTMLInputElement>(null);
  const [email, setEmail] = useState('');
  const [role, setRole] = useState('Member');
  const mutation = useMutation();
  return <Dialog title="Invite a person" busy={mutation.busy} status={mutation.status} close={close} initialFocus={emailInput}>
    <form className="ui-form" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
      await identityRequest(`${base}/invitations`, { email, role }); await saved();
    }); }}>
      <label className="ui-field">Email address<input ref={emailInput} name="email" type="email" required autoComplete="email" placeholder="name@company.com" value={email} onChange={event => setEmail(event.target.value)} /></label>
      <label className="ui-field">Workspace role<select name="role" value={role} onChange={event => setRole(event.target.value)}><option>Member</option>{viewerRole === 'Owner' && <option>Admin</option>}</select></label>
      <p className="ui-help">They’ll receive an email to join this workspace. Apps, Home panels and agent access are shared separately.</p>
      <Button type="submit" primary disabled={mutation.busy}>Send invitation</Button>
    </form>
  </Dialog>;
}
