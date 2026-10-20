import { canManageResources } from './workspace-context';
import { api, post } from './types';
import { element } from './view';
import { mountAppFrame, type AppSession } from './app-frame';
import { installAppRequest } from './app-request';
import './apps.css';
import { conversationUrl } from './agent-request';

type App = { id: string; agent_id: string; revision: number; active_revision: number | null; name: string; description: string; collections: Record<string, Record<string, string>> };
type Grant = { agent_id: string; collection: string; can_write: boolean };

export function installApps(): void {
  document.title = 'Apps · Relay';
  const root = document.querySelector<HTMLElement>('#app')!;
  root.innerHTML = `<main class="workspace-page apps-page" aria-label="Apps"><div class="apps-toolbar"><label class="app-search-label">Find an app<input type="search" id="app-search" placeholder="Find an app…"></label><button class="files-button" id="ask-app">＋ Create app</button></div>
    <p id="apps-status" role="status"></p>
    <div id="app-request" hidden></div><div id="apps-list"></div></main>`;
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
  function action(label: string, perform: () => Promise<void>): HTMLButtonElement {
    const button = element('button', 'files-button', label); button.onclick = async () => {
      if (busy) return; busy = true; button.disabled = true; status.textContent = '';
      try { await perform(); } catch (e) { status.textContent = (e as Error).message; }
      finally { busy = false; button.disabled = false; }
    }; return button;
  }
  async function open(app: App, preview: boolean): Promise<void> {
    const version = preview ? app.revision : app.active_revision;
    if (version === null) return;
    const session = await post<AppSession>(`/apps/${app.id}/session`, { revision: version, preview });
    const dialog = document.createElement('dialog'); dialog.className = 'app-viewer';
    dialog.setAttribute('aria-label', `${preview ? 'Preview' : 'Open'} ${app.name}`);
    const heading = element('div', 'panel-header'); heading.append(element('h2', '', `${app.name} · ${preview ? 'Ready to try' : 'Active'} revision ${version}`));
    const close = element('button', 'files-button', 'Close app'); heading.append(close);
    const state = element('p'); state.setAttribute('role', 'status'); state.textContent = 'Opening scoped app…';
    const source = document.createElement('details'); source.append(element('summary', '', 'Review source and records'));
    source.append(element('pre', '', JSON.stringify(session.definition, null, 2)));
    const surface = element('div', 'app-frame-surface'); dialog.append(heading, state, surface, source); root.append(dialog); dialog.showModal();
    const mounted = mountAppFrame(surface, app.id, session, state);
    close.onclick = () => dialog.close(); dialog.onclose = () => { mounted.dispose(); dialog.remove(); };
    if (preview) {
      const activate = action('Start using this app', async () => {
        await post(`/apps/${app.id}/activate`, { revision: version, preview_session: session.session, expected_active: app.active_revision });
        state.textContent = 'This exact release is active. Preview records remain separate; open the active app from Apps.'; activate.remove(); await refresh();
      }); activate.disabled = true;
      void mounted.ready.then(() => { if (dialog.open) activate.disabled = false; }); heading.append(activate);
    }
  }
  async function access(app: App, container: HTMLElement): Promise<void> {
    const [data, agents] = await Promise.all([api<{ grants: Grant[] }>(`/apps/${app.id}/grants`), api<{ agents: { profile: { id: string; name: string; state: string } }[] }>('/agents')]);
    const editor = element('section'); editor.append(element('h3', '', 'Agent record access'), element('p', '', 'Agents inherit no record access from creating an app. Choose a collection and grant read access explicitly; changes also need the agent’s exact action approval.'));
    const form = document.createElement('form'), agent = document.createElement('select'), collection = document.createElement('select');
    agent.setAttribute('aria-label', 'Agent for record access'); collection.setAttribute('aria-label', 'Record collection');
    for (const { profile } of agents.agents.filter(a => a.profile.state === 'ready')) { const option = document.createElement('option'); option.value = profile.id; option.textContent = profile.name; agent.append(option); }
    for (const key of Object.keys(app.collections)) { const option = document.createElement('option'); option.value = key; option.textContent = key; collection.append(option); }
    const label = document.createElement('label'), write = document.createElement('input'); write.type = 'checkbox'; label.append(write, document.createTextNode(' Also allow record changes'));
    const save = action('Assign record access', async () => { await post(`/apps/${app.id}/grants`, { agent_id: agent.value, collection: collection.value, enabled: true, can_write: write.checked }); await access(app, container); }); save.type = 'submit';
    form.onsubmit = event => event.preventDefault(); form.append(agent, collection, label, save); editor.append(form);
    for (const grant of data.grants) {
      const name = agents.agents.find(a => a.profile.id === grant.agent_id)?.profile.name ?? grant.agent_id;
      const row = element('p', '', `${name} · ${grant.collection} · ${grant.can_write ? 'read and change' : 'read only'} `);
      row.append(action('Remove record access', async () => { await post(`/apps/${app.id}/grants`, { ...grant, enabled: false }); await access(app, container); })); editor.append(row);
    }
    container.replaceChildren(editor);
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
        const management = document.createElement('details'); management.className = 'app-management';
        management.append(element('summary', '', 'Releases and access'), element('small', '', `Latest release ${app.revision} · ${app.active_revision === null ? 'not activated' : `active release ${app.active_revision}`}`));
        if (app.active_revision !== null) management.append(action('Preview latest release', () => open(app, true)));
        const agent = element('a', 'text-button', 'Ask its agent ↗'); agent.href = conversationUrl(app.agent_id, 'edit-app', app.id); if (canManageResources()) card.append(agent);
        const permissions = element('div');
        if (app.active_revision !== null) management.append(action('Manage agent access', () => access(app, permissions)));
        management.append(action('Archive app', async () => { await post(`/apps/${app.id}/archive`, { revision: app.revision }); await refresh(); }), permissions);
        if (canManageResources()) card.append(management); list.append(card);
      }
      filter();
    } catch (e) { status.textContent = (e as Error).message; }
    finally { loading = false; }
  }
  window.setInterval(() => { if (!busy && !root.querySelector('dialog[open]')) void refresh(); }, 5000);
  window.addEventListener('focus', () => { if (!busy) void refresh(); }); void refresh();
}
