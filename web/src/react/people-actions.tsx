import { useEffect, useRef, useState } from 'react';
import { identityRequest, workspaceId } from '../workspace-context';
import { Button, Dialog } from './ui';
import { useMutation } from './requests';
import { personName } from './remove-person-dialog';
import type { Member } from './people';

export type PersonAction = 'role' | 'access' | 'remove';

/** The ⋯ menu on a person's row. The only admin can't be made a member or removed. */
export function PersonMenu({ member, self, onlyAdmin, choose }: {
  member: Member; self: boolean; onlyAdmin: boolean; choose: (action: PersonAction) => void;
}) {
  const [open, setOpen] = useState(false);
  const toggle = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    menu.current?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    const outside = (event: MouseEvent) => { if (!menu.current?.contains(event.target as Node) && event.target !== toggle.current) setOpen(false); };
    document.addEventListener('mousedown', outside);
    return () => document.removeEventListener('mousedown', outside);
  }, [open]);
  // Focus returns to ⋯ first, so the dialog that opens hands it back there when it closes.
  const pick = (action: PersonAction) => { setOpen(false); toggle.current?.focus(); choose(action); };
  const admin = member.role === 'Admin';
  // Closes when focus moves elsewhere (another row's ⋯, Tab), not only on outside clicks.
  return <span className="settings-person-menu" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOpen(false); }}>
    <Button ref={toggle} className="ui-icon" aria-label={`Actions for ${personName(member)}`} aria-expanded={open} aria-haspopup="menu" onClick={() => setOpen(!open)}>⋯</Button>
    {open && <div ref={menu} className="chat-message-menu" role="menu" aria-label={`Actions for ${personName(member)}`}
      onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); setOpen(false); toggle.current?.focus(); } }}>
      <button type="button" role="menuitem" disabled={admin && onlyAdmin} onClick={() => pick('role')}>{admin ? 'Make member' : 'Make admin'}</button>
      <button type="button" role="menuitem" onClick={() => pick('access')}>Agent access…</button>
      {!self && <button type="button" role="menuitem" className="danger" disabled={admin && onlyAdmin} onClick={() => pick('remove')}>Remove from workspace…</button>}
      {admin && onlyAdmin && <p className="ui-help settings-menu-note">The only admin can’t be made a member or removed. Make someone else an admin first.</p>}
    </div>}
  </span>;
}

/** Confirms a role change; the server also keeps at least one admin. */
export function RoleDialog({ member, close, saved }: { member: Member; close: () => void; saved: (message: string) => Promise<void> }) {
  const mutation = useMutation();
  const toAdmin = member.role !== 'Admin';
  const name = personName(member);
  const save = () => void mutation.run(async () => {
    await identityRequest(`/workspaces/${workspaceId}/members/${member.id}`, { role: toAdmin ? 'Admin' : 'Member' });
    await saved(toAdmin ? `${name} is now an admin.` : `${name} is now a member.`);
  });
  return <Dialog title={toAdmin ? `Make ${name} an admin?` : `Make ${name} a member?`} busy={mutation.busy} status={mutation.status} close={close} footer={<>
    <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    <Button primary disabled={mutation.busy} onClick={save}>{toAdmin ? 'Make admin' : 'Make member'}</Button>
  </>}>
    <p className="ui-help">{toAdmin
      ? 'Admins manage people, connections and settings, and can make or remove other admins.'
      : 'They keep using what is shared with them, but can no longer manage people, connections or settings.'}</p>
  </Dialog>;
}
