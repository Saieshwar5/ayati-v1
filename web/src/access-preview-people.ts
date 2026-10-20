import { element } from './view';
import {
  cancelInvitation, changeMember, invite, membership,
  type Business, type Invitation, type PreviewState,
} from './access-preview-state';

type Button = (label: string, action: () => void, primary?: boolean) => HTMLButtonElement;

export function renderPeople(
  root: HTMLElement, state: PreviewState, business: Business, actor: string,
  act: (action: () => void) => void, button: Button,
): void {
  const ownRole = membership(business, actor)!.role;
  const manages = ownRole !== 'Member';
  root.append(element('h1', '', 'People'),
    element('p', 'access-subtitle', 'Everyone has their own account. Business resources are shared separately.'));
  if (manages) {
    const form = element('form', 'access-panel access-invite-form');
    form.append(element('h2', '', 'Invite someone'));
    const label = element('label', 'access-field', 'Email address');
    const email = element('input'); email.type = 'email'; email.required = true;
    email.placeholder = 'employee@example.com'; label.append(email);
    const roleLabel = element('label', 'access-field', 'Role');
    const role = element('select');
    for (const name of ownRole === 'Owner' ? ['Member', 'Admin'] : ['Member']) {
      const option = element('option', '', name); option.value = name; role.append(option);
    }
    roleLabel.append(role);
    const submit = element('button', 'access-button primary', 'Create demo invitation');
    form.append(label, roleLabel, submit,
      element('p', 'access-help', 'No email is sent. Open the demo link below and switch accounts to try joining.'));
    form.onsubmit = event => {
      event.preventDefault();
      act(() => { invite(state, business, actor, email.value, role.value as 'Admin' | 'Member'); });
    };
    root.append(form);
  }
  const list = element('section', 'access-panel');
  list.append(element('h2', '', `Members · ${business.members.length}`));
  for (const member of business.members) {
    const row = element('div', 'access-person');
    const details = element('div');
    details.append(element('strong', '', member.email),
      element('span', 'access-help', `${member.role} · Active${member.email === actor ? ' · You' : ''}`));
    row.append(details);
    if (manages && member.role !== 'Owner' && (ownRole === 'Owner' || member.role === 'Member')) {
      const actions = element('div', 'access-actions');
      if (ownRole === 'Owner') {
        const role = element('select');
        role.setAttribute('aria-label', `Role for ${member.email}`);
        for (const name of ['Member', 'Admin'] as const) {
          const option = element('option', '', name); option.value = name; role.append(option);
        }
        role.value = member.role;
        role.onchange = () => act(() => changeMember(business, actor, member.email, role.value as 'Admin' | 'Member'));
        actions.append(role);
      }
      actions.append(button('Remove access', () => {
        if (confirm(`Remove ${member.email} from ${business.name}? Business records will remain.`)) {
          changeMember(business, actor, member.email, null);
        }
      }));
      row.append(actions);
    }
    list.append(row);
  }
  root.append(list);
  if (!manages) return;
  const pending = state.invitations.filter(item => item.workspace === business.id && item.status === 'pending');
  const invitations = element('section', 'access-panel');
  invitations.append(element('h2', '', `Invitations · ${pending.length}`));
  if (!pending.length) invitations.append(element('p', 'access-help', 'No pending invitations.'));
  for (const invitation of pending) {
    const row = element('div', 'access-invitation');
    row.append(element('strong', '', invitation.email),
      element('p', 'access-help', `${invitation.role} · ${invitation.expires <= Date.now() ? 'Expired' : 'Invited'} · Expires ${new Date(invitation.expires).toLocaleDateString()}`));
    const actions = element('div', 'access-actions');
    const link = element('a', 'access-button', 'Open demo invitation');
    link.href = invitationUrl(invitation);
    actions.append(link, button('Resend', () => {
      // Validate that the actor may reissue this role before invalidating the old link.
      if (invitation.role === 'Admin' && ownRole !== 'Owner') {
        throw new Error('Only the owner can resend an admin invitation.');
      }
      cancelInvitation(business, actor, invitation);
      invite(state, business, actor, invitation.email, invitation.role);
    }), button('Cancel invitation', () => cancelInvitation(business, actor, invitation)));
    row.append(actions); invitations.append(row);
  }
  root.append(invitations);
}

function invitationUrl(invitation: Invitation): string {
  return `/?view=access-preview&invite=${encodeURIComponent(invitation.id)}`;
}
