import { brand } from './brand';
import { element } from './view';
import {
  accept, createBusiness, emptyState, emailAddress, membership,
  type Business, type PreviewState,
} from './access-preview-state';
import { renderPeople } from './access-preview-people';
import './access-preview.css';

const storageKey = 'ayati.access-preview.v1';
const sessionKey = 'ayati.access-preview.session';
type Session = { email: string; workspace: string };
type Screen = 'entry' | 'create' | 'choose' | 'home' | 'people' | 'account';

export function installAccessPreview(): void {
  const root = document.querySelector<HTMLDivElement>('#app')!;
  root.className = 'access-preview';
  document.title = 'Workspace preview · Relay';
  if (!['localhost', '127.0.0.1', '[::1]'].includes(location.hostname)) {
    root.textContent = 'The account preview is available only on localhost.';
    return;
  }
  let state = emptyState();
  let session: Session | null = null;
  let storageProblem = '';
  let screen: Screen = 'entry';
  let invitationId = new URLSearchParams(location.search).get('invite');
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved) {
      const parsed = JSON.parse(saved);
      if (!Array.isArray(parsed.businesses) || !Array.isArray(parsed.invitations)) throw new Error();
      state = parsed as PreviewState;
    }
    const savedSession = sessionStorage.getItem(sessionKey);
    if (savedSession) {
      const parsed = JSON.parse(savedSession);
      if (typeof parsed.email !== 'string' || typeof parsed.workspace !== 'string') throw new Error();
      session = parsed;
    }
  } catch { storageProblem = 'Saved preview data could not be read. Reset the demo to start again.'; }
  if (session) screen = session.workspace ? 'home' : 'choose';

  function save(): void {
    try {
      localStorage.setItem(storageKey, JSON.stringify(state));
      if (session) sessionStorage.setItem(sessionKey, JSON.stringify(session));
      else sessionStorage.removeItem(sessionKey);
    } catch { storageProblem = 'Browser storage is unavailable. Changes last only until this page closes.'; }
  }
  function act(action: () => void): void {
    try { action(); save(); render(); }
    catch (error) {
      const status = root.querySelector<HTMLElement>('[role=alert]')!;
      status.textContent = error instanceof Error ? error.message : 'Please try again.';
      status.focus();
    }
  }
  function button(text: string, action: () => void, primary = false): HTMLButtonElement {
    const node = element('button', primary ? 'access-button primary' : 'access-button', text);
    node.type = 'button'; node.onclick = () => act(action);
    return node;
  }
  function signOut(): void { session = null; screen = 'entry'; }
  function clearInvitation(): void {
    invitationId = null;
    history.replaceState(null, '', '/?view=access-preview');
  }
  function open(business: Business): void {
    if (!session || !membership(business, session.email)) throw new Error('You do not have access to this workspace.');
    session.workspace = business.id; screen = 'home';
  }
  function available(): Business[] {
    return state.businesses.filter(item => session && membership(item, session.email));
  }
  function heading(parent: HTMLElement, title: string, subtitle: string): void {
    parent.append(element('h1', '', title), element('p', 'access-subtitle', subtitle));
  }
  function field(form: HTMLElement, label: string, type = 'text'): HTMLInputElement {
    const wrapper = element('label', 'access-field', label);
    const input = element('input'); input.type = type; input.required = true;
    wrapper.append(input); form.append(wrapper);
    return input;
  }
  function render(): void {
    root.replaceChildren();
    const banner = element('div', 'access-demo');
    banner.append(element('span', '', 'Local preview · Simulated accounts · No emails sent'));
    const exit = element('a', '', 'Back to local Relay'); exit.href = '/';
    banner.append(exit);
    const error = element('p', 'access-error', storageProblem);
    error.setAttribute('role', 'alert'); error.tabIndex = -1;
    root.append(banner, error);
    if (invitationId) { renderInvitation(); return; }
    if (!session) { renderEntry(); return; }
    if (screen === 'create') { renderCreate(); return; }
    if (screen === 'choose' || !session.workspace) { renderChooser(); return; }
    const business = available().find(item => item.id === session!.workspace);
    if (!business) {
      const card = element('main', 'access-card');
      card.append(brand('access-brand'));
      heading(card, 'Workspace access unavailable', 'Your membership may have been removed. Choose another workspace or create one.');
      card.append(button('Choose workspace', () => { session!.workspace = ''; screen = 'choose'; }));
      root.append(card); return;
    }
    renderShell(business);
  }
  function renderEntry(): void {
    const card = element('main', 'access-card');
    card.append(brand('access-brand'));
    heading(card, 'Your business, together.', 'One place for you and your team to work with Relay.');
    const google = button('Continue with Google', () => {});
    google.disabled = true; google.title = 'Available after authentication provider setup';
    card.append(google, element('p', 'access-help', 'Google and email verification need provider setup. Use a demo identity below to explore.'));
    const form = element('form');
    const email = field(form, 'Demo email address', 'email');
    email.placeholder = 'owner@example.com'; email.autocomplete = 'email';
    const submit = element('button', 'access-button primary', 'Continue in preview');
    form.append(submit);
    form.onsubmit = event => {
      event.preventDefault();
      act(() => {
        session = { email: emailAddress(email.value), workspace: '' };
        const choices = available();
        if (!choices.length) screen = 'create';
        else if (choices.length === 1) open(choices[0]);
        else screen = 'choose';
      });
    };
    card.append(form, element('p', 'access-help', 'No password or verification code is collected. These identities cannot open real workspace data.'));
    root.append(card);
  }
  function renderCreate(): void {
    const card = element('main', 'access-card');
    card.append(brand('access-brand'));
    heading(card, 'Create your workspace', 'Start on your own. Invite your team whenever you’re ready.');
    const form = element('form');
    const name = field(form, 'Business or workspace name'); name.maxLength = 80; name.placeholder = 'Sai Stores';
    form.append(element('button', 'access-button primary', 'Create workspace'));
    form.onsubmit = event => {
      event.preventDefault();
      act(() => open(createBusiness(state, session!.email, name.value)));
    };
    card.append(form, button('Back', () => { screen = 'choose'; }), button('Sign out of demo', signOut));
    root.append(card);
  }
  function renderChooser(): void {
    const card = element('main', 'access-card');
    card.append(brand('access-brand'));
    heading(card, 'Choose a workspace', `Signed in as ${session!.email}`);
    const choices = available();
    if (!choices.length) card.append(element('p', '', 'You haven’t joined a workspace yet.'));
    for (const business of choices) {
      card.append(button(`${business.name} · ${membership(business, session!.email)!.role}`, () => open(business)));
    }
    card.append(button('Create a workspace', () => { screen = 'create'; }, true),
      button('Use another demo account', signOut));
    root.append(card);
  }
  function renderInvitation(): void {
    const card = element('main', 'access-card');
    card.append(brand('access-brand'));
    const invitation = state.invitations.find(item => item.id === invitationId);
    const business = state.businesses.find(item => item.id === invitation?.workspace);
    if (!invitation || !business || invitation.status === 'cancelled' || (invitation.status === 'pending' && invitation.expires <= Date.now())) {
      heading(card, 'Invitation unavailable', 'It may have expired or been cancelled. Demo links work only in this browser, on this address.');
      card.append(button('Continue to Relay', clearInvitation)); root.append(card); return;
    }
    heading(card, `Join ${business.name}`, `You’re invited as a ${invitation.role.toLowerCase()}.`);
    card.append(element('p', 'access-help', `Invited email: ${invitation.email}`));
    if (!session) {
      const form = element('form');
      const email = field(form, 'Demo email address', 'email');
      form.append(element('button', 'access-button primary', 'Continue to join'));
      form.onsubmit = event => {
        event.preventDefault(); act(() => {
          const actor = emailAddress(email.value);
          const joined = accept(state, invitation.id, actor);
          session = { email: actor, workspace: joined.id };
          screen = 'home'; clearInvitation();
        });
      };
      card.append(form);
    } else if (session.email !== invitation.email) {
      card.append(element('p', '', `You’re using ${session.email}. Switch to the invited account to continue.`),
        button('Switch demo account', signOut));
    } else {
      card.append(button(invitation.status === 'accepted' ? 'Open workspace' : 'Accept invitation', () => {
        open(accept(state, invitation.id, session!.email)); clearInvitation();
      }, true));
    }
    card.append(button('Not now', clearInvitation));
    root.append(card);
  }
  function renderShell(business: Business): void {
    const layout = element('div', 'access-layout');
    const rail = element('aside', 'access-sidebar');
    rail.append(brand('access-wordmark'),
      button(`${business.name} ▾`, () => { screen = 'choose'; }));
    const nav = element('nav'); nav.setAttribute('aria-label', 'Preview workspace');
    for (const [label, target] of [['Home', 'home'], ['People', 'people'], ['My account', 'account']] as const) {
      const item = button(label, () => { screen = target; });
      if (screen === target) item.setAttribute('aria-current', 'page');
      nav.append(item);
    }
    rail.append(nav, element('p', 'access-help', session!.email), button('Sign out', signOut));
    const content = element('main', 'access-content');
    if (screen === 'people') {
      renderPeople(content, state, business, session!.email, act, button);
    } else if (screen === 'account') {
      heading(content, 'My account', 'Your identity is separate from the businesses you belong to.');
      const card = element('section', 'access-panel');
      card.append(element('h2', '', session!.email), element('p', '', 'Demo identity · Not verified'),
        element('h3', '', 'Security'), element('p', 'access-help', 'Google sign-in, email verification, passkeys and session management will be available after provider integration.'),
        button('Use another demo account', signOut),
        button('Reset preview data', () => {
          if (!confirm('Delete all demo workspaces, members and invitations? Your real local workspace is unaffected.')) return;
          state = emptyState(); session = null; storageProblem = ''; screen = 'entry';
        }));
      content.append(card);
    } else {
      heading(content, `Welcome to ${business.name}`, 'A place for your business to work together.');
      const card = element('section', 'access-panel access-welcome');
      card.append(element('span', 'access-eyebrow', 'YOUR WORKSPACE IS READY'),
        element('h2', '', 'Start with you. Grow with your team.'),
        element('p', '', 'Keep working on your own, or invite an employee to share this workspace.'));
      if (membership(business, session!.email)!.role !== 'Member') {
        card.append(button('Invite your team', () => { screen = 'people'; }, true));
      }
      card.append(element('p', 'access-help', 'This preview contains only membership screens. Agents, Apps and business files are intentionally not connected.'));
      content.append(card);
    }
    layout.append(rail, content); root.append(layout);
  }
  window.addEventListener('storage', event => {
    if (event.key !== storageKey) return;
    try { state = event.newValue ? JSON.parse(event.newValue) : emptyState(); render(); }
    catch { storageProblem = 'Preview data changed unexpectedly. Reload or reset the demo.'; render(); }
  });
  render();
}
