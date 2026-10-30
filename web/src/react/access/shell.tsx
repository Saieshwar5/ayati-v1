import { useState, type ReactNode } from 'react';
import { Brand } from '../ui';
import type { AccountUser } from '../display-name-form';
import { initials } from './model';

/** Top bar for signed-in access pages: logo, and the account with sign out. */
export function AccessShell({ user, signOut, busy, children }: {
  user: AccountUser; signOut: () => void; busy: boolean; children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const name = user.display_name || user.email;
  return <div className="access-page">
    <header className="access-top">
      <a href="/?view=account&choose=1" aria-label="Cuckoo home"><Brand className="access-logo" /></a>
      <div className="access-account">
        <button type="button" className="access-account-button" aria-expanded={open} aria-controls="access-account-menu" onClick={() => setOpen(!open)}>
          <span className="access-avatar" aria-hidden="true">{initials(name)}</span>
          <span className="access-account-text"><strong>{name}</strong><small>{user.email}</small></span>
        </button>
        {open && <div id="access-account-menu" className="access-menu">
          <button type="button" disabled={busy} onClick={signOut}>Sign out</button>
        </div>}
      </div>
    </header>
    <main className="access-main">{children}</main>
  </div>;
}

/** A plain notice with a coloured edge, for early-access and state messages. */
export function Notice({ title, children }: { title: string; children: ReactNode }) {
  return <div className="access-notice"><strong>{title}</strong><span>{children}</span></div>;
}
