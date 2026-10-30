import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { DisplayNameForm } from './display-name-form';
import { workspaceId } from '../workspace-context';
import { errorMessage, useMutation } from './requests';
import { accessRequest, SignedOut, type Invitation, type Session } from './access/model';
import { AccessShell } from './access/shell';
import { SignIn } from './access/sign-in';
import { WorkspaceList } from './access/workspace-list';
import { EnterCode } from './access/enter-code';
import { CreateWorkspace } from './access/create-workspace';
import { AdminCodes } from './access/admin-codes';

type Screen = { kind: 'home' } | { kind: 'code' } | { kind: 'create'; code?: string; expires?: number } | { kind: 'admin' };
const lastKey = (user: string) => `relay.last-workspace.${user}`;

/** Sign in, choose or create a workspace (invite-only early access), and the account view. */
export function AccountPage({ embedded = false }: { embedded?: boolean }) {
  const [session, setSession] = useState<Session>();
  const [screen, setScreen] = useState<Screen>(() => new URLSearchParams(location.search).has('admin') ? { kind: 'admin' } : { kind: 'home' });
  const [notice, setNotice] = useState('');
  const mutation = useMutation();

  // Initial loading is read-only; Strict Mode may repeat it without accepting or creating anything.
  const load = useCallback(async (initial: boolean) => {
    const response = await fetch('/api/identity/session', { credentials: 'same-origin' });
    const data: Session & { error?: string } = await response.json();
    if (!response.ok && response.status !== 401) throw new Error(data.error || 'Unable to load account.');
    const next = response.status === 401 ? { user: null } : data;
    setSession(next);
    if (!initial) return;
    const params = new URLSearchParams(location.search);
    if (embedded) {
      if (!next.user) location.assign('/?view=account');
      else if (!next.workspaces?.some(w => w.id === workspaceId)) location.assign('/?view=account&choose=1');
      return;
    }
    if (params.has('login_error')) setNotice('Sign-in was not completed. Try again, and make sure your email is verified.');
    if (!params.has('choose') && !params.has('admin') && !next.invitations?.length && next.user?.display_name) {
      let selected = next.workspaces?.length === 1 ? next.workspaces[0].id : '';
      try { const last = localStorage.getItem(lastKey(next.user.id)); if (!selected && next.workspaces?.some(w => w.id === last)) selected = last!; } catch { /* Optional. */ }
      if (selected) location.assign(`/w/${selected}/`);
    }
  }, [embedded]);
  useEffect(() => {
    let active = true;
    document.title = embedded ? 'My account · Cuckoo' : 'Your workspaces · Cuckoo';
    load(true).catch(error => { if (active) { setNotice(errorMessage(error)); setSession({ user: null }); } });
    return () => { active = false; };
  }, [embedded, load]);

  const signOut = () => void mutation.run(async () => {
    const result = await accessRequest<{ provider_session_revoked: boolean }>('/logout', {});
    setSession({ user: null }); setScreen({ kind: 'home' });
    if (!result.provider_session_revoked) setNotice('Signed out of Cuckoo. WorkOS session termination could not be confirmed.');
    if (embedded) location.assign('/?view=account');
  });
  const guarded = (run: () => Promise<void>) => void mutation.run(async () => {
    try { await run(); } catch (error) { if (error instanceof SignedOut) setSession({ user: null }); throw error; }
  });
  const accept = (invitation: Invitation) => guarded(async () => {
    const joined = await accessRequest<{ id: string }>(`/invitations/${invitation.id}/accept`, {});
    location.assign(`/w/${joined.id}/`);
  });
  const decline = (invitation: Invitation) => guarded(async () => {
    await accessRequest(`/invitations/${invitation.id}/decline`, {}); await load(false);
  });

  if (!session) return <p className="access-loading" role="status">Checking your session…</p>;
  if (!session.user) return <SignIn notice={mutation.status || notice} />;
  const user = session.user;
  const shell = (content: ReactNode) => <AccessShell user={user} signOut={signOut} busy={mutation.busy}>
    {(mutation.status || notice) && <p className="access-alert" role="alert">{mutation.status || notice}</p>}{content}</AccessShell>;

  if (!user.display_name) return shell(<div className="access-column narrow">
    <h1>What should people call you?</h1><p className="access-lead">Choose a display name to get started.</p>
    <DisplayNameForm key={user.id} user={user} saved={next => setSession(current => current ? { ...current, user: next } : current)} />
  </div>);
  if (embedded) return <main className="access-content"><h1>My account</h1><p className="access-lead">{user.email}</p>
    <DisplayNameForm key={user.id} user={user} saved={() => location.assign(location.href)} />
    <div className="access-actions"><a className="access-button" href="/?view=account&choose=1">Your workspaces</a>
      <button type="button" className="access-button" disabled={mutation.busy} onClick={signOut}>Sign out</button></div>
    {mutation.status && <p className="access-alert" role="alert">{mutation.status}</p>}
  </main>;

  const admin = Boolean(session.platform_admin);
  const workspaces = session.workspaces ?? [], invitations = session.invitations ?? [];
  const home = () => { setScreen({ kind: 'home' }); setNotice(''); };
  if (screen.kind === 'admin' && admin) return shell(<AdminCodes back={home} />);
  if (screen.kind === 'create' && (screen.code || admin)) return shell(<CreateWorkspace code={screen.code} expires={screen.expires} back={home} />);
  const empty = !workspaces.length && !invitations.length;
  if (screen.kind === 'code' || (empty && !admin)) return shell(<EnterCode email={user.email} empty={empty}
    accepted={(code, expires) => setScreen({ kind: 'create', code, expires })} back={empty ? undefined : home}
    refresh={() => guarded(() => load(false))} />);
  let last: string | undefined;
  try { last = localStorage.getItem(lastKey(user.id)) ?? undefined; } catch { /* Optional. */ }
  return shell(<WorkspaceList name={user.display_name} workspaces={workspaces} invitations={invitations} last={last}
    busy={mutation.busy} accept={accept} decline={decline} admin={admin}
    startCode={() => { setNotice(''); setScreen(admin ? { kind: 'create' } : { kind: 'code' }); }} />);
}
