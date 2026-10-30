import { useEffect, useState, type ReactNode } from 'react';
import { Brand } from '../ui';
import { errorMessage, useMutation } from '../requests';
import { accessRequest, days, initials, tileColor, type Session } from './model';
import { Notice } from './shell';

type Preview = {
  status: 'pending' | 'accepted' | 'expired' | 'withdrawn' | 'invalid';
  workspace?: string; inviter?: string | null; role?: string; expires?: number; email_hint?: string;
  matches?: boolean; workspace_id?: string;
};

/** /invite/<secret>: see the invitation, sign in with the invited email, accept, set up a profile. */
export function InvitePage({ token }: { token: string }) {
  const [preview, setPreview] = useState<Preview>();
  const [session, setSession] = useState<Session>();
  const [joined, setJoined] = useState<string>();
  const [error, setError] = useState('');
  const mutation = useMutation();
  const back = `/invite/${token}`;

  useEffect(() => {
    let active = true;
    document.title = 'Invitation · Cuckoo';
    // Read-only: opening the link never accepts it.
    Promise.all([
      accessRequest<Preview>(`/invite-links/${encodeURIComponent(token)}`),
      fetch('/api/identity/session', { credentials: 'same-origin' }).then(r => r.ok ? r.json() as Promise<Session> : { user: null }),
    ]).then(([next, who]) => { if (active) { setPreview(next); setSession(who); } })
      .catch(failure => { if (active) setError(errorMessage(failure)); });
    return () => { active = false; };
  }, [token]);

  const signIn = (method?: 'google') => `/auth/login?return_to=${encodeURIComponent(back)}${method ? `&method=${method}` : ''}`;
  const signOut = () => void mutation.run(async () => { await accessRequest('/logout', {}); location.assign(signIn()); });
  const user = session?.user;
  const card = (body: ReactNode) => <div className="access-invite"><div className="access-invite-card">
    <Brand className="access-logo" />{body}</div></div>;

  if (error) return card(<><h1>This invitation could not be opened</h1><p className="access-alert" role="alert">{error}</p></>);
  if (!preview || !session) return <p className="access-loading" role="status">Opening your invitation…</p>;
  if (joined) return card(<ProfileSetup workspace={joined} name={user?.display_name || user?.email.split('@')[0] || ''} />);

  if (preview.status !== 'pending' && !(preview.status === 'accepted' && preview.workspace_id)) {
    const why = preview.status === 'expired' ? 'This invitation has expired.' : preview.status === 'accepted' ? 'This invitation has already been used.' : 'This invitation link is not valid or was withdrawn.';
    return card(<><h1>{why}</h1><p className="access-lead">Ask the person who invited you to send a new invitation.</p>
      <a className="access-button" href="/?view=account&choose=1">Go to Cuckoo</a></>);
  }
  const heading = <>
    <span className="access-tile large" style={{ background: tileColor(preview.workspace ?? '') }} aria-hidden="true">{initials(preview.workspace ?? '')}</span>
    <h1>{preview.inviter ? `${preview.inviter} invited you to ${preview.workspace}` : `You’re invited to ${preview.workspace}`}</h1>
    <p className="access-lead">Join as {preview.role === 'Owner' ? 'an owner' : 'a member'}{preview.expires ? ` · expires in ${days(preview.expires)} ${days(preview.expires) === 1 ? 'day' : 'days'}` : ''}</p>
  </>;
  if (preview.status === 'accepted' && preview.workspace_id) return card(<>{heading}
    <Notice title="You already joined">Open the workspace to continue.</Notice>
    <a className="access-button primary" href={`/w/${preview.workspace_id}/`}>Open {preview.workspace}</a></>);

  if (!user) return card(<>{heading}
    <p className="access-help">Sign in with <strong>{preview.email_hint}</strong>, the address this invitation was sent to.</p>
    <a className="access-button" href={signIn('google')}><span className="access-google" aria-hidden="true">G</span>Continue with Google</a>
    <a className="access-button primary" href={signIn()}>Continue with email</a>
    <p className="access-fine">Sign-in by WorkOS · Cuckoo never sees your Google password.</p></>);

  if (!preview.matches) return card(<>{heading}
    <Notice title={`This invitation is for ${preview.email_hint}`}>You are signed in as {user.email}. Sign in with the invited address to join.</Notice>
    <button type="button" className="access-button primary" disabled={mutation.busy} onClick={signOut}>Use another account</button>
    {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}</>);

  return card(<>{heading}
    <p className="access-help">Signed in as <strong>{user.email}</strong>.</p>
    <button type="button" className="access-button primary" disabled={mutation.busy} onClick={() => void mutation.run(async () => {
      const result = await accessRequest<{ id: string }>(`/invite-links/${encodeURIComponent(token)}/accept`, {});
      setJoined(result.id);
    })}>Accept and join</button>
    <button type="button" className="access-button quiet" disabled={mutation.busy} onClick={() => void mutation.run(async () => {
      await accessRequest(`/invite-links/${encodeURIComponent(token)}/decline`, {});
      setPreview(current => current && { ...current, status: 'withdrawn' });
    })}>Decline</button>
    {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}</>);
}

/** First visit after joining: how colleagues will see you in this workspace. */
function ProfileSetup({ workspace, name }: { workspace: string; name: string }) {
  const [displayName, setDisplayName] = useState(name);
  const [title, setTitle] = useState('');
  const mutation = useMutation();
  const open = () => location.assign(`/w/${workspace}/`);
  return <form onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
    await accessRequest(`/workspaces/${workspace}/my-profile`, { display_name: displayName, title, about: '' });
    open();
  }); }}>
    <h1>Set up your profile</h1>
    <p className="access-lead">This is how your colleagues and agents will see you. You can add a photo later in Settings.</p>
    <fieldset className="ui-fieldset" disabled={mutation.busy}>
      <label className="access-field">Your name<input required maxLength={80} autoComplete="name" value={displayName} onChange={event => setDisplayName(event.target.value)} /></label>
      <label className="access-field">Job title <small>(optional)</small><input maxLength={80} autoComplete="organization-title" placeholder="Operations lead" value={title} onChange={event => setTitle(event.target.value)} /></label>
      <button type="submit" className="access-button primary">{mutation.busy ? 'Saving…' : 'Continue to workspace'}</button>
    </fieldset>
    {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}
  </form>;
}
