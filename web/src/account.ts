import { workspaceId } from './workspace-context';
import { brand } from './brand';
import { element } from './view';
import './access-preview.css';

type Workspace = { id: string; name: string; role: string };
type Invite = { id: string; name: string; role: string };
type Session = { user: { id: string; email: string } | null; workspaces?: Workspace[]; invitations?: Invite[] };
export async function installAccount(embedded?: 'account'): Promise<void> {
  const root = document.querySelector<HTMLDivElement>('#app')!;
  root.className = 'access-preview';
  document.title = embedded === 'account' ? 'My account · Relay' : 'Your workspaces · Relay';
  let session: Session = { user: null };
  let selected = embedded ? workspaceId : '';
  let screen: 'home' | 'create' | 'choose' | 'account' = embedded ?? 'home';
  let busy = false;
  let notice = '';
  let createId = crypto.randomUUID();
  async function api<T>(path: string, body?: unknown): Promise<T> {
    const response = await fetch(`/api/identity${path}`, {
      method: body === undefined ? 'GET' : 'POST',
      headers: body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Ayati-Request': 'identity' },
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'same-origin',
    });
    const data = await response.json();
    if (response.status === 401) { session = { user: null }; selected = ''; }
    if (!response.ok) throw new Error(data.error || 'Request could not be completed.');
    return data as T;
  }
  function button(label: string, action: () => Promise<void> | void, primary = false): HTMLButtonElement {
    const node = element('button', `access-button${primary ? ' primary' : ''}`, label);
    node.type = 'button';
    node.onclick = () => { void run(action); };
    return node;
  }
  async function run(action: () => Promise<void> | void): Promise<void> {
    if (busy) return;
    busy = true;
    root.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; });
    try { notice = ''; await action(); await render(); }
    catch (error) {
      notice = error instanceof Error ? error.message : 'Please try again.';
      if (!session.user) await render();
      const alert = root.querySelector<HTMLElement>('[role=alert]');
      if (alert) { alert.textContent = notice; alert.focus(); }
    } finally {
      busy = false;
      root.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = false; });
    }
  }
  async function refresh(): Promise<void> {
    session = await api<Session>('/session');
    if (!session.workspaces?.some(w => w.id === selected)) {
      selected = session.workspaces?.length === 1 ? session.workspaces[0].id : '';
    }
  }
  function heading(parent: HTMLElement, title: string, detail: string): void {
    parent.append(element('h1', '', title), element('p', 'access-subtitle', detail));
  }
  function field(parent: HTMLElement, title: string, type = 'text'): HTMLInputElement {
    const label = element('label', 'access-field', title);
    const input = element('input'); input.type = type; input.required = true;
    label.append(input); parent.append(label); return input;
  }
  async function logout(): Promise<void> {
    const result = await api<{ provider_session_revoked: boolean }>('/logout', {});
    session = { user: null }; selected = '';
    if (!result.provider_session_revoked) notice = 'Signed out of Relay. WorkOS session termination could not be confirmed.';
  }
  async function render(): Promise<void> {
    root.replaceChildren();
    const banner = element('div', 'access-demo', 'Relay · Your business workspace');
    if (!embedded) root.append(banner);
    const error = element('p', 'access-error', notice); error.setAttribute('role', 'alert'); error.tabIndex = -1; root.append(error);
    if (embedded && !session.user) { location.assign('/?view=account'); return; }
    if (embedded && session.user) {
      const workspace = session.workspaces?.find(w => w.id === workspaceId);
      if (!workspace) { location.assign('/?view=account&choose=1'); return; }
      const content = element('main', 'access-content'); root.append(content);
      heading(content, 'My account', session.user.email);
      content.append(button('Manage workspaces', () => location.assign('/?view=account&choose=1')),
        button('Sign out', async () => { await logout(); location.assign('/?view=account'); }));
      return;
    }
    if (!session.user) {
      const card = element('main', 'access-card');
      card.append(brand('access-brand'));
      heading(card, 'Your business, together.', 'Sign in securely, then create your workspace or join your team.');
      const link = element('a', 'access-button primary', 'Continue to sign in');
      link.href = '/auth/login';
      card.append(link, element('p', 'access-help', 'Your available sign-in methods are managed by WorkOS. Relay never asks for your Google password.'));
      root.append(card); return;
    }
    if (screen === 'create') {
      const card = element('main', 'access-card');
      card.append(brand('access-brand'));
      heading(card, 'Create your workspace', 'Start on your own. Invite employees whenever you’re ready.');
      const form = element('form');
      const name = field(form, 'Business or workspace name'); name.maxLength = 80;
      form.append(element('button', 'access-button primary', 'Create workspace'));
      form.onsubmit = event => {
        event.preventDefault(); void run(async () => {
          const result = await api<{ id: string }>('/workspaces', { name: name.value, request_id: createId });
          selected = result.id; createId = crypto.randomUUID(); screen = embedded ?? 'home'; await refresh();
        });
      };
      card.append(form, button('Back', () => { screen = 'choose'; }));
      root.append(card); return;
    }
    if (!selected || screen === 'choose') {
      const card = element('main', 'access-card');
      card.append(brand('access-brand'));
      heading(card, 'Your workspaces', session.user.email);
      for (const invitation of session.invitations ?? []) {
        const section = element('section', 'access-panel');
        section.append(element('h2', '', `Invitation to ${invitation.name}`),
          element('p', 'access-help', `Join as a ${invitation.role.toLowerCase()}.`),
          button('Accept invitation', async () => {
            const joined = await api<{ id: string }>(`/invitations/${invitation.id}/accept`, {});
            selected = joined.id; screen = embedded ?? 'home'; await refresh();
          }, true));
        card.append(section);
      }
      for (const workspace of session.workspaces ?? []) {
        card.append(button(`${workspace.name} · ${workspace.role}`, () => { selected = workspace.id; screen = embedded ?? 'home'; }));
      }
      card.append(button('Create a workspace', () => { screen = 'create'; }, true), button('Sign out', logout));
      root.append(card); return;
    }
    location.assign(`/w/${selected}/`);
  }
  root.append(element('p', 'access-subtitle', 'Checking your session…'));
  try {
    await refresh();
    if (!embedded) {
      const params = new URLSearchParams(location.search);
      if (params.has('choose') || session.invitations?.length) { screen = 'choose'; selected = ''; }
      else if (session.user && !selected) {
        try { const last = localStorage.getItem(`relay.last-workspace.${session.user.id}`); if (session.workspaces?.some(w => w.id === last)) selected = last!; } catch { /* Optional. */ }
      }
    }
    if (new URLSearchParams(location.search).has('login_error')) notice = 'Sign-in was not completed. Try again, and make sure your email is verified.';
    await render();
  } catch (error) {
    notice = error instanceof Error ? error.message : 'Unable to load account.';
    await render();
  }
}
