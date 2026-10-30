import { useRef, useState } from 'react';
import { identityRequest, workspaceId, workspaceSession } from '../workspace-context';
import { Button, Dialog, Tabs } from './ui';
import { useMutation, useResource } from './requests';
import { MemberDetails } from './member-details';
import { PersonFace } from './faces';
import { PersonMenu, RoleDialog, type PersonAction } from './people-actions';
import { RemovePersonDialog, personName } from './remove-person-dialog';

export type Member = { id: string; email: string; display_name?: string | null; role: string };
type People = { role: string; members: Member[]; invitations: { id: string; email: string; role: string; status: string; expires: number }[] };
const base = `/workspaces/${workspaceId}`;
const loadPeople = () => identityRequest<People>(`${base}/people`);

export function PeoplePage() {
  const { data, error, refresh } = useResource(loadPeople);
  const [query, setQuery] = useState('');
  const [inviting, setInviting] = useState(false);
  const [open, setOpen] = useState<{ action: PersonAction; member: Member }>();
  const mutation = useMutation();
  const you = workspaceSession?.user?.id;
  const adminCount = data?.members.filter(person => person.role === 'Admin').length ?? 0;
  const matches = (email: string) => email.toLowerCase().includes(query.trim().toLowerCase());
  async function saved(message: string) {
    mutation.setStatus(message);
    try { await refresh(); } catch { mutation.setStatus(`${message} Reload People to refresh the list.`); }
  }
  const members = data?.members.filter(member => matches(member.email) || matches(member.display_name ?? '')) ?? [];
  const invitations = data?.invitations.filter(invitation => matches(invitation.email)) ?? [];
  if (data && data.role !== 'Admin') return <p role="alert">Only admins can manage workspace access.</p>;
  return <div className="settings-people"><main className="desk-page" aria-label="People">
    <header className="desk-toolbar"><div className="desk-title"><h2>People</h2><p>Who belongs to this workspace and who manages it.</p></div>
      <input type="search" className="ui-search" placeholder="Find a person" aria-label="Find a person" value={query} onChange={event => setQuery(event.target.value)} />
      <Button primary hidden={data?.role !== 'Admin'} onClick={() => setInviting(true)}>＋ Invite person</Button>
    </header><p className="ui-status" role="status">{mutation.status || error || (!data ? 'Loading your team…' : '')}</p>
    {error && <Button onClick={() => void refresh().catch(() => {})}>Retry</Button>}
    <div className="desk-content"><Tabs sections={[
      { title: `People · ${data?.members.length ?? 0}`, content: <div className="ui-directory">{members.map(member => <div className="ui-row settings-person" key={member.id}>
        <PersonFace person={{ id: member.id, name: personName(member) }} className="ui-avatar" />
        <div className="ui-row-title"><strong>{personName(member)}{member.id === you && <small className="settings-you"> (you)</small>}</strong>{member.display_name && <small>{member.email}</small>}</div>
        <span className={`ui-badge${member.role === 'Admin' ? ' settings-admin' : ''}`}>{member.role}</span>
        <PersonMenu member={member} self={member.id === you} onlyAdmin={member.role === 'Admin' && adminCount < 2} choose={action => setOpen({ action, member })} />
      </div>)}{data && !members.length && <p className="ui-empty">{query ? 'No members match your search.' : 'No members to show.'}</p>}</div> },
      { title: `Invitations · ${data?.invitations.length ?? 0}`, content: <div className="ui-directory">{invitations.map(invitation => <div className="ui-row" key={invitation.id}>
        <div className="ui-row-title"><strong>{invitation.email}</strong><small>{invitation.role}</small></div>
        <span className="ui-badge">{invitation.status === 'delivery_unknown' ? 'Delivery unconfirmed' : invitation.expires * 1000 <= Date.now() ? 'Expired' : invitation.status}</span>
        {data?.role === 'Admin' && invitation.status === 'pending' && invitation.expires * 1000 > Date.now() && <Button disabled={mutation.busy}
          aria-label={`Copy a new invite link for ${invitation.email}`} onClick={() => void mutation.run(async () => {
            // Links are stored hashed, so copying makes a new one and the old link stops working.
            const result = await identityRequest<{ link: string }>(`${base}/invitations/${invitation.id}/link`, {});
            await navigator.clipboard?.writeText(result.link);
            mutation.setStatus(`New invite link copied for ${invitation.email}. Earlier links for this invitation no longer work.`);
          })}>Copy new link</Button>}
        {data?.role === 'Admin' && <Button disabled={mutation.busy} onClick={() => {
          if (!confirm(`Cancel the invitation for ${invitation.email}?`)) return;
          void mutation.run(async () => {
            const result = await identityRequest<{ provider_invitation_revoked: boolean }>(`${base}/invitations/${invitation.id}/cancel`, {});
            await saved(result.provider_invitation_revoked ? 'Invitation cancelled.' : 'Workspace invitation cancelled. The email link may still allow account signup, but cannot grant workspace access.');
          });
        }}>Cancel invitation</Button>}
      </div>)}{data && !invitations.length && <p className="ui-empty">{query ? 'No invitations match your search.' : 'No invitations to show. Invite someone when you’re ready.'}</p>}</div> },
    ]} /></div>
    {inviting && data && <InviteDialog close={() => setInviting(false)} failed={() => void refresh().catch(() => {})} saved={async () => { setInviting(false); await saved('Invitation sent. They can open the link, sign in with that email and accept.'); }} />}
    {open?.action === 'access' && <MemberDetails key={open.member.id} member={open.member} close={() => setOpen(undefined)} />}
    {open?.action === 'role' && <RoleDialog key={open.member.id} member={open.member} close={() => setOpen(undefined)} saved={async message => { setOpen(undefined); await saved(message); }} />}
    {open?.action === 'remove' && data && <RemovePersonDialog key={open.member.id} member={open.member} members={data.members} close={() => setOpen(undefined)}
      removed={async message => { setOpen(undefined); await saved(message); }} />}
    <p className="ui-help settings-people-note">A workspace always keeps at least one admin. Removing someone hands their shared agents and Library items to the person you choose.</p>
  </main></div>;
}

function InviteDialog({ close, saved, failed }: { close: () => void; saved: () => Promise<void>; failed: () => void }) {
  const emailInput = useRef<HTMLInputElement>(null);
  const [email, setEmail] = useState('');
  const [link, setLink] = useState('');
  const [copied, setCopied] = useState(false);
  const mutation = useMutation();
  // After sending, the link is shown once so the owner can also send it on WhatsApp or chat.
  if (link) return <Dialog title="Invitation sent" close={() => void saved()}>
    <p className="ui-help">{email} received an email. You can also send this link; it works for {email} only and expires in 7 days.</p>
    <p className="invite-link"><code>{link}</code></p>
    <div className="access-actions"><Button primary onClick={() => void navigator.clipboard?.writeText(link).then(() => setCopied(true))}>{copied ? 'Link copied' : 'Copy invite link'}</Button>
      <Button onClick={() => void saved()}>Done</Button></div>
  </Dialog>;
  return <Dialog title="Invite a person" busy={mutation.busy} status={mutation.status} close={close} initialFocus={emailInput}>
    <form className="ui-form" onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
      let result: { link?: string };
      try { result = await identityRequest<{ id: string; link?: string }>(`${base}/invitations`, { email, role: 'Member' }); }
      catch (error) { failed(); throw error; }
      if (result.link) setLink(result.link); else await saved();
    }); }}>
      <label className="ui-field">Email address<input ref={emailInput} name="email" type="email" required autoComplete="email" placeholder="name@example.com" value={email} onChange={event => setEmail(event.target.value)} /></label>
      <p className="ui-help">They’ll receive an email to join as a member. An admin can make them an admin after they join. Apps, Home panels and agent access are shared separately.</p>
      <Button type="submit" primary disabled={mutation.busy}>Send invitation</Button>
    </form>
  </Dialog>;
}
