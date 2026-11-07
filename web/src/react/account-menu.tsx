// The account menu at the foot of the rail (designs/chat-and-account screen 9, D245):
// only you, your workspaces, Settings and Sign out. Profile and theme live in Settings.
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { currentWorkspace, workspaceSession, workspaceId, workspaceUrl, identityRequest } from '../workspace-context';
import { initials, tileColor } from './access/model';
import { chatRequest, type Directory } from './chat/model';
import { PersonFace } from './faces';
import { useMutation } from './requests';
import { activeStatus, applyPresence, usePresence } from './chat/presence';
import { clearsPhrase, PresenceDot, statusEmoji, statusLabel } from './chat/presence-view';
import { StatusDialog } from './chat/status-dialog';

type IconName = 'check' | 'workspaces' | 'settings' | 'sign-out';
function MenuIcon({ name }: { name: IconName }) {
  return <svg className="account-menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
    {name === 'check' && <path d="m5 12.5 4.5 4.5L19 7.5" />}
    {name === 'workspaces' && <><path d="M4 21V4h10v17M14 9h6v12M2 21h20" /><path d="M7.5 8h3M7.5 12h3M7.5 16h3M17 13v.01M17 17v.01" /></>}
    {name === 'settings' && <><path d="M3 6h18M3 12h18M3 18h18" /><circle cx="8" cy="6" r="2" /><circle cx="16" cy="12" r="2" /><circle cx="9" cy="18" r="2" /></>}
    {name === 'sign-out' && <><path d="M14 4H5v16h9" /><path d="M10 12h11m-4-4 4 4-4 4" /></>}
  </svg>;
}

/** The session has your name and email; your photo comes from the workspace directory.
 * It is read when the shell opens and again each time the menu opens, so a photo changed
 * in Settings → Profile shows without a reload. Older reads are discarded. The same read
 * brings your status and presence into the presence store. */
function useMyPhoto(userId: string | undefined, reads: number) {
  const [photo, setPhoto] = useState<string | null>(null);
  useEffect(() => {
    if (!userId) return;
    const abort = new AbortController();
    chatRequest<Directory>('/chat-directory', undefined, abort.signal).then(directory => {
      if (abort.signal.aborted) return;
      applyPresence(directory.presence);
      setPhoto(directory.users?.find(person => person.id === userId)?.photo ?? null);
    }).catch(() => { /* Initials stay when the directory is unavailable. */ });
    return () => abort.abort();
  }, [userId, reads]);
  return photo;
}

/** Your status under your name (screen 9, note 2); it opens Set a status. */
function StatusLine({ choose }: { choose: () => void }) {
  const status = activeStatus(usePresence(workspaceSession?.user?.id));
  const ends = status ? clearsPhrase(status) : '';
  return <button type="button" className="workspace-menu-action account-menu-status" onClick={choose}
    aria-label={status ? `Status: ${statusLabel(status)}${ends ? `, ${ends}` : ''}. Change status` : 'Set a status'}>
    <span className="account-menu-status-mark" aria-hidden="true">{status ? statusEmoji(status) : <span className="presence-dot inline" />}</span>
    <span className="account-menu-text"><strong>{status ? statusLabel(status) : 'Set a status'}</strong>{' '}
      <small>{status ? `${ends ? `${ends} · ` : ''}change` : 'Available'}</small></span>
    <svg className="account-menu-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m9 6 6 6-6 6" /></svg>
  </button>;
}

export function AccountMenu() {
  const workspace = currentWorkspace();
  const user = workspaceSession?.user ?? undefined;
  const [open, setOpen] = useState(false);
  const [choosing, setChoosing] = useState(false);
  const [reads, setReads] = useState(0);
  const photo = useMyPhoto(user?.id, reads);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLElement>(null);
  const mutation = useMutation();
  if (!workspaceId || !workspace || !user) return null;
  const email = user.email;
  const name = user.display_name || email;
  const me = { id: user.id, name, photo };
  const toggled = (state: string) => {
    setOpen(state === 'open');
    if (state === 'open') setReads(count => count + 1);
  };
  // The browser closes the popover on Escape too; closing here also returns focus to your photo.
  const close = () => { menu.current?.hidePopover?.(); trigger.current?.focus(); };
  // The menu closes first, so the dialog returns focus to your photo.
  const chooseStatus = () => { close(); setChoosing(true); };
  const signOut = () => void mutation.run(async () => {
    try { await identityRequest('/logout', {}); location.assign('/?view=account'); }
    catch { throw new Error('Sign-out failed. Please try again.'); }
  });
  return <>
    <button ref={trigger} type="button" title="Your account" className="workspace-menu-trigger workspace-profile-trigger" aria-label="Your account"
      aria-controls="workspace-profile" aria-expanded={open} popoverTarget="workspace-profile">
      <PersonFace person={me} className="workspace-profile-avatar" badge={<PresenceDot id={user.id} />} />
    </button>
    {createPortal(<section ref={menu} id="workspace-profile" className="workspace-menu workspace-profile" popover="auto" aria-label="Your account"
      onToggle={event => toggled((event.nativeEvent as ToggleEvent).newState)}
      onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); close(); } }}>
      <div className="account-menu-identity">
        <PersonFace person={me} className="account-menu-face" badge={<PresenceDot id={user.id} />} />
        <p><strong>{name}</strong>{' '}<span>{email}</span></p>
      </div>
      <StatusLine choose={chooseStatus} />
      <nav className="account-menu-group" aria-labelledby="account-menu-workspaces">
        <h2 id="account-menu-workspaces">Workspaces</h2>
        <ul className="workspace-menu-list">{workspaceSession?.workspaces?.map(entry => <li key={entry.id}>
          <a className="workspace-menu-action account-menu-workspace" href={`/w/${entry.id}/`} aria-current={entry.id === workspaceId ? 'true' : undefined}>
            <span className="account-menu-tile" style={{ background: tileColor(entry.id) }} aria-hidden="true">{initials(entry.name)}</span>
            <span className="account-menu-text"><strong>{entry.name}</strong>{' '}<small>{entry.role}</small></span>
            {entry.id === workspaceId && <MenuIcon name="check" />}
          </a></li>)}</ul>
        <a className="workspace-menu-action" href="/?view=account&choose=1"><MenuIcon name="workspaces" />All workspaces…</a>
      </nav>
      <div className="account-menu-group">
        {workspaceSession?.simulation && <a className="workspace-menu-action" href="/index.html?view=simulation">Simulation · switch employee</a>}
        <a className="workspace-menu-action" href={workspaceUrl('/?view=settings&section=profile')}><MenuIcon name="settings" />Settings</a>
        <button type="button" className="workspace-menu-action" disabled={mutation.busy} onClick={signOut}><MenuIcon name="sign-out" />Sign out</button>
        <p className="workspace-menu-status" role="status">{mutation.status}</p>
      </div>
    </section>, document.body)}
    {choosing && <StatusDialog close={() => setChoosing(false)} />}
  </>;
}
