import { currentView } from './view-lifetime';
import { canManageResources } from './workspace-context';

import { element } from './view';
import { mountAppFrame, type AppSession } from './app-frame';
import { installAppRequest } from './app-request';
import './apps.css';
import { conversationUrl } from './agent-request';
import { createDialog, installTabs } from './app-ui';

type App = { id: string; agent_id: string; revision: number; active_revision: number | null; name: string; description: string; collections: Record<string, Record<string, string>> };
type Grant = { agent_id: string; collection: string; can_write: boolean };

export function installApps(): void {
  const view = currentView();
  const { api, post } = view;
  document.title = 'Apps · Relay';
  const root = view.querySelector<HTMLElement>('#app')!;
  root.innerHTML = `<main class="desk-page apps-page" aria-label="Apps"><header class="desk-toolbar"><div class="desk-title"><h1>Apps</h1><p>Your everyday tools, ready to open.</p></div><input class="ui-search" type="search" id="app-search" aria-label="Find an app" placeholder="Find an app"><button class="ui-button primary" id="ask-app">＋ Create app</button></header>
    <p id="apps-status" role="status"></p><div class="desk-content"><div id="app-request" hidden></div><div id="apps-list"></div></div></main>`;
  installAppRequest(root);
  root.querySelector<HTMLButtonElement>('#ask-app')!.hidden = !canManageResources();
  const list = root.querySelector<HTMLElement>('#apps-list')!;
  const status = root.querySelector<HTMLElement>('#apps-status')!;
  const search = root.querySelector<HTMLInputElement>('#app-search')!;
  const noMatch = element('p', 'apps-empty', 'No apps match your search.');
  noMatch.hidden = true; list.after(noMatch);
  function filter(): void {
    const query = search.value.trim().toLocaleLowerCase();
    const cards = [...list.querySelectorAll<HTMLElement>('.app-card')];
    cards.forEach(card => { card.hidden = !card.dataset.search?.includes(query); });
    noMatch.hidden = !cards.length || cards.some(card => !card.hidden);
  }
  search.oninput = filter;
  let busy = false, loading = false, fingerprint = '';
  function action(label: string, perform: () => Promise<void>, feedback = status, setBusy: (busy: boolean) => void = () => {}): HTMLButtonElement {
    const button = element('button', 'ui-button', label); button.type = 'button'; button.onclick = async () => {
      if (busy) return; busy = true; button.disabled = true; feedback.textContent = ''; setBusy(true);
      try { await perform(); } catch (e) { feedback.textContent = (e as Error).message; }
      finally { busy = false; button.disabled = false; setBusy(false); }
    }; return button;
  }
  async function open(app: App, preview: boolean): Promise<void> {
    const version = preview ? app.revision : app.active_revision;
    if (version === null) return;
    const session = await post<AppSession>(`/apps/${app.id}/session`, { revision: version, preview });
    const dialog = document.createElement('dialog'); dialog.className = 'app-viewer';
    dialog.setAttribute('aria-label', `${preview ? 'Preview' : 'Open'} ${app.name}`);
    const heading = element('div', 'panel-header'); heading.append(element('h2', '', preview ? `${app.name} · Preview` : app.name));
    const close = element('button', 'files-button', 'Close app'); heading.append(close);
    const state = element('p'); state.setAttribute('role', 'status'); state.textContent = 'Opening app…';
    const source = document.createElement('details'); source.append(element('summary', '', 'Review source and records'));
    source.append(element('pre', '', JSON.stringify(session.definition, null, 2)));
    const surface = element('div', 'app-frame-surface'); dialog.append(heading, state, surface, source); root.append(dialog); dialog.showModal();
    const mounted = mountAppFrame(surface, app.id, session, state);
    let activating = false;
    dialog.addEventListener('cancel', event => { if (activating) event.preventDefault(); });
    close.onclick = () => { if (!activating) dialog.close(); }; dialog.onclose = () => { mounted.dispose(); dialog.remove(); };
    if (preview) {
      const activate = action('Start using this app', async () => {
        activating = true; close.disabled = true;
        try { await post(`/apps/${app.id}/activate`, { revision: version, preview_session: session.session, expected_active: app.active_revision });
        state.textContent = 'This exact release is active. Preview records remain separate; open the active app from Apps.'; activate.remove(); await refresh(); }
        finally { activating = false; close.disabled = false; }
      }, state); activate.disabled = true;
      void mounted.ready.then(() => { if (dialog.open) activate.disabled = false; }); heading.append(activate);
    }
  }
  async function access(app: App, container: HTMLElement, feedback: HTMLElement, setBusy: (busy: boolean) => void): Promise<void> {
    const [data, agents] = await Promise.all([api<{ grants: Grant[] }>(`/apps/${app.id}/grants`), api<{ agents: { profile: { id: string; name: string; state: string } }[] }>('/agents')]);
    const editor = element('section', 'app-access-editor'); editor.append(element('h3', '', 'Agent record access'), element('p', '', 'Agents inherit no record access from creating an app. Choose a collection and grant read access explicitly; changes also need the agent’s exact action approval.'));
    const form = element('form', 'ui-form'), agent = document.createElement('select'), collection = document.createElement('select');
    agent.setAttribute('aria-label', 'Agent for record access'); collection.setAttribute('aria-label', 'Record collection');
    for (const { profile } of agents.agents.filter(a => a.profile.state === 'ready')) { const option = document.createElement('option'); option.value = profile.id; option.textContent = profile.name; agent.append(option); }
    for (const key of Object.keys(app.collections)) { const option = document.createElement('option'); option.value = key; option.textContent = key; collection.append(option); }
    const agentLabel = element('label', 'ui-field', 'Agent'), collectionLabel = element('label', 'ui-field', 'Record collection');
    agentLabel.append(agent); collectionLabel.append(collection);
    const label = element('label', 'ui-check'), write = document.createElement('input'); write.type = 'checkbox'; label.append(write, document.createTextNode(' Also allow record changes'));
    const save = action('Assign record access', async () => { await post(`/apps/${app.id}/grants`, { agent_id: agent.value, collection: collection.value, enabled: true, can_write: write.checked }); feedback.textContent = 'Agent record access saved.'; await access(app, container, feedback, setBusy); }, feedback, setBusy); save.type = 'button';
    save.disabled = !agent.options.length || !collection.options.length;
    form.onsubmit = event => event.preventDefault(); form.append(agentLabel, collectionLabel, label, save); editor.append(form);
    for (const grant of data.grants) {
      const name = agents.agents.find(a => a.profile.id === grant.agent_id)?.profile.name ?? grant.agent_id;
      const row = element('p', '', `${name} · ${grant.collection} · ${grant.can_write ? 'read and change' : 'read only'} `);
      row.append(action('Remove record access', async () => { await post(`/apps/${app.id}/grants`, { ...grant, enabled: false }); feedback.textContent = 'Agent record access saved.'; await access(app, container, feedback, setBusy); }, feedback, setBusy)); editor.append(row);
    }
    container.replaceChildren(editor);
  }
  function manage(app: App): void {
    const sheet = createDialog(app.name, true);
    sheet.body.append(element('p', 'ui-help', app.description));
    const releases = element('section', 'ui-form');
    releases.append(element('p', 'ui-help', `Latest release ${app.revision} · ${app.active_revision === null ? 'Not active yet' : `Active release ${app.active_revision}`}`));
    releases.append(action('Preview latest release', () => open(app, true), sheet.status, sheet.setBusy));
    const agent = element('a', 'ui-button', 'Ask its agent to make changes');
    agent.href = conversationUrl(app.agent_id, 'edit-app', app.id); releases.append(agent);
    releases.append(action('Archive app', async () => {
      if (!confirm(`Archive ${app.name}? It will be removed from the launcher.`)) return;
      sheet.setBusy(true);
      try { await post(`/apps/${app.id}/archive`, { revision: app.revision }); sheet.dialog.close(); await refresh(); }
      finally { sheet.setBusy(false); }
    }, sheet.status));
    const permissions = element('section');
    permissions.append(element('p', 'ui-help', 'People’s access is managed from Users. These controls manage agent access to records.'));
    if (app.active_revision !== null) permissions.append(action('Load agent access', () => access(app, permissions, sheet.status, sheet.setBusy), sheet.status, sheet.setBusy));
    else permissions.append(element('p', 'ui-help', 'Activate a release before assigning record access.'));
    installTabs(sheet.body, [{ title: 'Releases', content: releases }, { title: 'Agent access', content: permissions }]);
    sheet.show();
  }
  async function refresh(): Promise<void> {
    if (loading || document.hidden) return; loading = true;
    try {
      const data = await api<{ apps: App[]; available: boolean }>('/apps');
      const next = JSON.stringify(data); if (next === fingerprint) return; fingerprint = next;
      list.replaceChildren();
      status.textContent = data.available ? '' : 'Apps require Goose execution.';
      if (!data.apps.length) list.append(element('p', 'home-empty', canManageResources() ? 'Ask an agent to build the internal tool your work needs.' : 'No apps have been shared with you. Business-resource access is currently owner-controlled.'));
      for (const app of data.apps) {
        const card = element('article', 'app-card'); card.dataset.app = app.id;
        card.dataset.search = `${app.name} ${app.description}`.toLocaleLowerCase();
        const heading = element('div', 'app-card-heading');
        const mark = element('span', 'app-mark', app.name.slice(0, 2).toUpperCase()); mark.setAttribute('aria-hidden', 'true');
        heading.append(mark, element('h2', '', app.name), element('span', 'app-release-state', app.active_revision === null ? 'Ready to try' : 'Active'));
        card.append(heading, element('p', 'app-description', app.description));
        const launch = action(app.active_revision !== null ? 'Open app' : 'Preview latest release', () => open(app, app.active_revision === null));
        launch.classList.add('app-launch'); card.append(launch);
        if (canManageResources()) card.append(action('Manage', async () => manage(app)));
        list.append(card);
      }
      filter();
    } catch (e) { status.textContent = (e as Error).message; }
    finally { loading = false; }
  }
  view.interval(() => { if (!busy && !view.querySelector('dialog[open]')) void refresh(); }, 5000);
  view.onWindow('focus', () => { if (!busy && !view.querySelector('dialog[open]')) void refresh(); }); void refresh();
}
