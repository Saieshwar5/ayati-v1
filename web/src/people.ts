import { identityRequest, workspaceId } from './workspace-context.ts';
import { element } from './view.ts';
import { createDialog, installTabs, searchInput, uiButton } from './app-ui.ts';
import { openMember } from './member-details.ts';

export type Member = { id: string; email: string; role: string };
export type People = { role: string; members: Member[];
  invitations: { id: string; email: string; role: string; status: string; expires: number }[] };

export async function installPeople(): Promise<void> {
  document.title = 'Users · Relay';
  const root = document.querySelector<HTMLElement>('#app')!;
  root.innerHTML = `<main class="desk-page" aria-label="Users">
    <header class="desk-toolbar"><div class="desk-title"><h1>Users</h1><p>Your team and their workspace access.</p></div></header>
    <p class="ui-status" role="status"></p><div class="desk-content"></div></main>`;
  const toolbar = root.querySelector<HTMLElement>('.desk-toolbar')!;
  const content = root.querySelector<HTMLElement>('.desk-content')!;
  const status = root.querySelector<HTMLElement>('[role=status]')!;
  const search = searchInput('Find a person');
  const invite = uiButton('＋ Invite person', () => openInvite(), true);
  invite.hidden = true; toolbar.append(search, invite);
  const members = element('section', 'ui-surface');
  const invitations = element('section', 'ui-surface');
  installTabs(content, [{ title: 'Members', content: members }, { title: 'Invitations', content: invitations }]);
  let data: People;
  const base = `/workspaces/${workspaceId}`;
  function draw(): void {
    const query = search.value.trim().toLowerCase();
    const scroll = content.scrollTop;
    members.replaceChildren(); invitations.replaceChildren();
    for (const member of data.members.filter(m => m.email.toLowerCase().includes(query))) {
      const row = element('div', 'ui-row');
      const avatar = element('span', 'ui-avatar', member.email.slice(0, 2).toUpperCase()); avatar.setAttribute('aria-hidden', 'true');
      const detail = element('div', 'ui-row-title'); detail.append(element('strong', '', member.email));
      const view = uiButton('Details', () => openMember(member, data.role, refresh));
      view.dataset.member = member.id;
      view.setAttribute('aria-label', `View ${member.email}`);
      row.append(avatar, detail, element('span', 'ui-badge', member.role), view); members.append(row);
    }
    if (!members.childElementCount) members.append(element('p', 'ui-empty', query ? 'No members match your search.' : 'No members to show.'));
    for (const invitation of data.invitations.filter(i => i.email.toLowerCase().includes(query))) {
      const row = element('div', 'ui-row');
      const detail = element('div', 'ui-row-title');
      detail.append(element('strong', '', invitation.email), element('small', '', invitation.role));
      row.append(detail, element('span', 'ui-badge', invitation.expires * 1000 <= Date.now() ? 'Expired' : invitation.status));
      if (data.role !== 'Member') {
        const cancel = uiButton('Cancel invitation', () => {
          if (!confirm(`Cancel the invitation for ${invitation.email}?`)) return;
          cancel.disabled = true;
          void identityRequest<{ provider_invitation_revoked: boolean }>(`${base}/invitations/${invitation.id}/cancel`, {})
            .then(async result => {
              status.textContent = result.provider_invitation_revoked ? 'Invitation cancelled.' : 'Workspace invitation cancelled. The email link may still allow account signup, but cannot grant workspace access.';
              await refresh();
            }).catch(error => { status.textContent = error.message; }).finally(() => { cancel.disabled = false; });
        });
        row.append(cancel);
      }
      invitations.append(row);
    }
    if (!invitations.childElementCount) invitations.append(element('p', 'ui-empty', query ? 'No invitations match your search.' : 'No invitations to show. Invite someone when you’re ready.'));
    content.scrollTop = scroll;
  }
  async function refresh(): Promise<void> {
    data = await identityRequest<People>(`${base}/people`);
    invite.hidden = data.role === 'Member'; draw();
  }
  function openInvite(): void {
    const sheet = createDialog('Invite a person');
    const form = element('form', 'ui-form'); form.id = `invite-${crypto.randomUUID()}`;
    form.innerHTML = `<label class="ui-field">Email address<input name="email" type="email" required autocomplete="email" placeholder="name@company.com"></label>
      <label class="ui-field">Workspace role<select name="role"><option>Member</option></select></label>
      <p class="ui-help">They’ll receive an email to join this workspace. Apps, Home panels and agent access are shared separately.</p>`;
    const role = form.elements.namedItem('role') as HTMLSelectElement;
    if (data.role === 'Owner') role.append(new Option('Admin', 'Admin'));
    const submit = element('button', 'ui-button primary', 'Send invitation');
    submit.type = 'submit'; submit.setAttribute('form', form.id);
    sheet.body.append(form); sheet.footer.append(submit); sheet.show();
    (form.elements.namedItem('email') as HTMLInputElement).focus();
    let pending = false;
    form.onsubmit = async event => {
      event.preventDefault(); if (pending) return;
      pending = true; submit.disabled = true; sheet.setBusy(true); sheet.status.textContent = 'Sending invitation…';
      try {
        await identityRequest(`${base}/invitations`, { email: (form.elements.namedItem('email') as HTMLInputElement).value, role: role.value });
        sheet.dialog.close(); status.textContent = 'Invitation sent. They can sign in and accept it under Your workspaces.';
        try { await refresh(); } catch { status.textContent += ' Refresh to see the updated list.'; }
      } catch (error) { sheet.status.textContent = (error as Error).message; }
      finally { pending = false; submit.disabled = false; sheet.setBusy(false); }
    };
  }
  search.oninput = () => { if (data) draw(); };
  status.textContent = 'Loading your team…';
  try { await refresh(); status.textContent = ''; }
  catch (error) { status.textContent = (error as Error).message; toolbar.append(uiButton('Retry', () => { void refresh().then(() => { status.textContent = ''; }).catch(e => { status.textContent = e.message; }); })); }
}
