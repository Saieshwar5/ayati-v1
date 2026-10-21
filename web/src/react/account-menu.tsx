import { useState } from 'react';
import { createPortal } from 'react-dom';
import { currentWorkspace, workspaceSession, workspaceId, workspaceUrl, identityRequest } from '../workspace-context';
import { useMutation } from './requests';

export function AccountMenu() {
  const workspace = currentWorkspace();
  const [open, setOpen] = useState(false);
  const mutation = useMutation();
  if (!workspaceId || !workspace) return null;
  const email = workspaceSession?.user?.email ?? '';
  const name = workspaceSession?.user?.display_name || email;
  const initials = name.split(/[\s@._-]+/).filter(Boolean).slice(0, 2).map(part => Array.from(part)[0]).join('').toUpperCase() || 'W';
  return <><button type="button" title="Your account" className="workspace-menu-trigger workspace-profile-trigger" aria-label="Your account"
    aria-controls="workspace-profile" aria-expanded={open} popoverTarget="workspace-profile"><span className="workspace-profile-avatar" aria-hidden="true">{initials}</span></button>
    {createPortal(<div id="workspace-profile" className="workspace-menu workspace-profile" popover="auto" aria-label="Your account"
      onToggle={event => setOpen((event.nativeEvent as ToggleEvent).newState === 'open')}>
      <h2>{name}</h2><p className="workspace-menu-email">{email}</p><p className="workspace-menu-role">{workspace.name} · {workspace.role}</p><hr />
      {workspaceSession?.simulation && <a className="workspace-menu-action" href="/index.html?view=simulation">Simulation · switch employee</a>}
      <h2>Workspaces</h2><nav className="workspace-menu-list" aria-label="Choose workspace">{workspaceSession?.workspaces?.map(entry =>
        <a key={entry.id} className="workspace-menu-action" href={`/w/${entry.id}/`} aria-current={entry.id === workspaceId ? 'true' : undefined}>
          {entry.name}{entry.id === workspaceId && <small>Current</small>}</a>)}</nav>
      <a className="workspace-menu-action" href="/?view=account&choose=1">Manage or create workspaces</a><hr />
      <a className="workspace-menu-action" href={workspaceUrl('/?view=account')}>My account</a>
      <a className="workspace-menu-action" href={workspaceUrl('/?view=settings#theme')}>Appearance</a><hr />
      <button type="button" className="workspace-menu-action" disabled={mutation.busy} onClick={() => void mutation.run(async () => {
        try { await identityRequest('/logout', {}); location.assign('/?view=account'); }
        catch { throw new Error('Sign-out failed. Please try again.'); }
      })}>Sign out</button><p className="workspace-menu-status" role="status">{mutation.status}</p>
    </div>, document.body)}</>;
}
