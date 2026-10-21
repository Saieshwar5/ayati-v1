import { useEffect, useRef, useState } from 'react';
import { DisplayNameForm, type AccountUser } from './display-name-form';
import { workspaceId } from '../workspace-context';
import { Brand } from './ui';
import { errorMessage, useMutation } from './requests';

type Workspace = { id: string; name: string; role: string };
type Session = { user: AccountUser | null; workspaces?: Workspace[]; invitations?: Workspace[] };

export function AccountPage({ embedded = false }: { embedded?: boolean }) {
  const [session, setSession] = useState<Session>();
  const [screen, setScreen] = useState<'choose' | 'create'>('choose');
  const [name, setName] = useState('');
  const [notice, setNotice] = useState('');
  const createId = useRef('');
  if (!createId.current) createId.current = crypto.randomUUID();
  const mutation = useMutation();
  async function request<T>(path: string, body?: unknown): Promise<T> {
    const response = await fetch(`/api/identity${path}`, {
      method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin',
      headers: body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Ayati-Request': 'identity' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const data = await response.json();
    if (response.status === 401) setSession({ user: null });
    if (!response.ok) throw new Error(data.error || 'Request could not be completed.');
    return data;
  }
  useEffect(() => {
    let active = true;
    document.title = embedded ? 'My account · Relay' : 'Your workspaces · Relay';
    // Initial loading is read-only; Strict Mode may repeat it without accepting an invitation or creating a workspace.
    void fetch('/api/identity/session', { credentials: 'same-origin' }).then(async response => {
      const data: Session & { error?: string } = await response.json();
      if (!active) return;
      if (!response.ok && response.status !== 401) throw new Error(data.error || 'Unable to load account.');
      const next = response.status === 401 ? { user: null } : data;
      setSession(next);
      if (embedded) {
        if (!next.user) location.assign('/?view=account');
        else if (!next.workspaces?.some(w => w.id === workspaceId)) location.assign('/?view=account&choose=1');
      } else {
        const params = new URLSearchParams(location.search);
        if (params.has('login_error')) setNotice('Sign-in was not completed. Try again, and make sure your email is verified.');
        if (!params.has('choose') && !next.invitations?.length && next.user?.display_name) {
          let selected = next.workspaces?.length === 1 ? next.workspaces[0].id : '';
          try { const last = localStorage.getItem(`relay.last-workspace.${next.user.id}`); if (!selected && next.workspaces?.some(w => w.id === last)) selected = last!; } catch { /* Optional. */ }
          if (selected) location.assign(`/w/${selected}/`);
        }
      }
    }).catch(error => { if (active) { setNotice(errorMessage(error)); setSession({ user: null }); } });
    return () => { active = false; };
  }, [embedded]);
  async function logout() {
    const result = await request<{ provider_session_revoked: boolean }>('/logout', {});
    setSession({ user: null });
    if (!result.provider_session_revoked) setNotice('Signed out of Relay. WorkOS session termination could not be confirmed.');
    if (embedded) location.assign('/?view=account');
  }
  const action = (label: string, run: () => Promise<void>, primary = false) =>
    <button type="button" className={`access-button${primary ? ' primary' : ''}`} disabled={mutation.busy} onClick={() => void mutation.run(run)}>{label}</button>;
  return <div id="app" className="access-preview">
    {!embedded && <div className="access-demo">Relay · Your business workspace</div>}
    <p className="access-error" role="alert">{mutation.status || notice}</p>
    {!session ? <p className="access-subtitle">Checking your session…</p> : !session.user ? <main className="access-card">
      <Brand className="access-brand" /><h1>Your business, together.</h1><p className="access-subtitle">Sign in securely, then create your workspace or join your team.</p>
      <a className="access-button primary" href="/auth/login">Continue to sign in</a>
      <p className="access-help">Your available sign-in methods are managed by WorkOS. Relay never asks for your Google password.</p>
    </main> : !session.user.display_name ? <main className="access-card">
      <Brand className="access-brand" /><h1>What should people call you?</h1>
      <p className="access-subtitle">Choose a display name to get started.</p>
      <DisplayNameForm key={session.user.id} user={session.user} saved={user => setSession(current => current ? { ...current, user } : current)} />
      {action('Sign out', logout)}
    </main> : embedded ? <main className="access-content"><h1>My account</h1><p className="access-subtitle">{session.user.email}</p>
      <DisplayNameForm key={session.user.id} user={session.user} saved={() => location.assign(location.href)} />
      {action('Manage workspaces', async () => location.assign('/?view=account&choose=1'))}{action('Sign out', logout)}
    </main> : screen === 'create' ? <main className="access-card"><Brand className="access-brand" />
      <h1>Create your workspace</h1><p className="access-subtitle">Start on your own. Invite employees whenever you’re ready.</p>
      <form onSubmit={event => { event.preventDefault(); void mutation.run(async () => {
        const result = await request<{ id: string }>('/workspaces', { name, request_id: createId.current });
        location.assign(`/w/${result.id}/`);
      }); }}><fieldset className="ui-fieldset" disabled={mutation.busy}>
        <label className="access-field">Business or workspace name<input name="name" required maxLength={80} value={name} onChange={event => setName(event.target.value)} /></label>
        <button type="submit" className="access-button primary">Create workspace</button>
      </fieldset></form>
      <button type="button" className="access-button" disabled={mutation.busy} onClick={() => setScreen('choose')}>Back</button>
    </main> : <main className="access-card"><Brand className="access-brand" /><h1>Your workspaces</h1><p className="access-subtitle">{session.user.display_name} · {session.user.email}</p>
      <details><summary>Edit display name</summary><DisplayNameForm key={session.user.id} user={session.user} saved={user => setSession(current => current ? { ...current, user } : current)} /></details>
      {session.invitations?.map(invitation => <section key={invitation.id} className="access-panel"><h2>Invitation to {invitation.name}</h2>
        <p className="access-help">Join as a {invitation.role.toLowerCase()}.</p>{action('Accept invitation', async () => {
          const joined = await request<{ id: string }>(`/invitations/${invitation.id}/accept`, {});
          location.assign(`/w/${joined.id}/`);
        }, true)}</section>)}
      {session.workspaces?.map(workspace => <div key={workspace.id}>{action(`${workspace.name} · ${workspace.role}`, async () => location.assign(`/w/${workspace.id}/`))}</div>)}
      <button type="button" className="access-button primary" disabled={mutation.busy} onClick={() => { setNotice(''); setScreen('create'); }}>Create a workspace</button>
      {action('Sign out', logout)}
    </main>}
  </div>;
}
