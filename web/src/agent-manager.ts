import { displayAgentName } from './agents';
import { currentView } from './view-lifetime';

import { canManageResources, workspaceUrl } from './workspace-context.ts';
import { element } from './view.ts';
import { createDialog, installTabs, searchInput, uiButton } from './app-ui.ts';
import type { Profile } from './agents';
type Directory = { agents: { profile: Profile; activity: { pending_tasks: number } }[]; creation_available: boolean; default_model: string; memory_available: boolean; browser_available: boolean };

export async function installAgentManager(): Promise<void> {
  const view = currentView();
  const { api, post } = view;
  const root = view.querySelector<HTMLDivElement>('#app')!;
  document.title = 'Agents · Cuckoo';
  root.innerHTML = `<main class="desk-page" aria-label="Agents"><header class="desk-toolbar"><div class="desk-title"><h1>Agents</h1><p>Your team’s agents. Their conversations live in Chat.</p></div></header><p class="ui-status" role="status"></p><div class="desk-content"><div class="manager-list ui-directory"></div></div></main>`;
  const toolbar = root.querySelector<HTMLElement>('.desk-toolbar')!;
  const list = root.querySelector<HTMLElement>('.manager-list')!;
  const status = root.querySelector<HTMLElement>('[role=status]')!;
  if (!canManageResources()) { list.append(element('p', 'ui-empty', 'Your workspace owner manages agents. Open Chat to talk with agents shared with you.')); return; }
  const create = uiButton('＋ Create agent', () => open(), true); create.disabled = true;
  const search = searchInput('Find an agent'); toolbar.append(search, create);
  let directory: Directory; let fingerprint = ''; let editing = false;
  function open(profile?: Profile): void {
    const sheet = createDialog(profile ? profile.name : 'Create an agent', true);
    editing = true; sheet.dialog.addEventListener('close', () => { editing = false; });
    const form = element('form'); form.id = `agent-${crypto.randomUUID()}`;
    const overview = element('section', 'ui-form');
    overview.innerHTML = `<label class="ui-field">Name<input name="name" required maxlength="80" placeholder="e.g. Operations assistant"></label>
      <label class="ui-field">Responsibilities<textarea name="role" required maxlength="4000" placeholder="What should this agent help with?"></textarea></label>`;
    const capabilities = element('section');
    capabilities.innerHTML = `<p class="ui-help">Available capabilities still follow their existing approval checks.</p>
      <label class="ui-check"><input type="checkbox" name="memory"><span>Sourced memory<br><small class="ui-help">Keep useful context with its sources.</small></span></label>
      <label class="ui-check"><input type="checkbox" name="browser"><span>Browser<br><small class="ui-help">Work with websites when a browser is available.</small></span></label>`;
    const advanced = element('section', 'ui-form');
    advanced.innerHTML = `<label class="ui-field">Model<input name="model" required maxlength="200"></label>
      <p class="ui-help">Changing responsibilities or capabilities starts fresh model context. Saved history remains.</p>`;
    const sections = [overview, capabilities, advanced];
    const selectTab = installTabs(form, [{ title: 'Overview', content: overview }, { title: 'Capabilities', content: capabilities }, { title: 'Advanced', content: advanced }]);
    // Reveal an invalid field before the browser moves focus to it.
    form.addEventListener('invalid', event => { const index = sections.findIndex(s => s.contains(event.target as Node)); if (index >= 0) selectTab(index); }, true);
    sheet.body.append(form);
    const field = (name: string) => form.elements.namedItem(name) as HTMLInputElement;
    field('name').value = profile?.name ?? ''; field('role').value = profile?.role ?? '';
    field('model').value = profile?.model ?? directory.default_model;
    const canEdit = !profile || (profile.editable && profile.state === 'ready');
    for (const name of ['memory', 'browser'] as const) {
      field(name).checked = profile?.[name] ?? false;
      field(name).disabled = !canEdit || (name === 'browser' ? !(profile?.browser_available ?? directory.browser_available) && !profile?.browser : !directory.memory_available && !profile?.memory);
    }
    for (const name of ['name', 'role', 'model']) field(name).disabled = !canEdit;
    const save = element('button', 'ui-button primary', profile ? 'Save changes' : 'Create agent');
    save.type = 'submit'; save.setAttribute('form', form.id); save.disabled = !canEdit;
    const requestId = crypto.randomUUID(); let saving = false;
    form.onsubmit = async event => {
      event.preventDefault(); if (saving || !canEdit) return;
      saving = true; save.disabled = true; sheet.setBusy(true); sheet.status.textContent = 'Saving…';
      const input = { name: field('name').value, role: field('role').value, model: field('model').value, memory: field('memory').checked, browser: field('browser').checked };
      try {
        await post(profile ? `/agents/${profile.id}/profile` : '/agents', profile ? { ...input, expected_revision: profile.revision } : { ...input, request_id: requestId });
        sheet.dialog.close(); await refresh();
      } catch (error) { sheet.status.textContent = (error as Error).message; }
      finally { saving = false; save.disabled = false; sheet.setBusy(false); }
    };
    if (canEdit) sheet.footer.append(save);
    if (profile) {
      advanced.append(element('p', 'ui-help', `Status: ${profile.state}${profile.detail ? ` · ${profile.detail}` : ''}`));
      const actions = profile.id === 'local-agent' ? [] : profile.state === 'archived' ? ['restore', 'delete'] : profile.state === 'failed' ? ['retry', 'archive'] : profile.state === 'ready' || profile.state === 'archiving' ? ['archive'] : [];
      for (const name of actions) {
        const button = uiButton(`${name[0].toUpperCase() + name.slice(1)} agent`, () => {
          if (saving) return;
          if (name === 'delete' && !confirm(`Remove ${profile.name} from the directory? Saved files, history and Apps are retained.`)) return;
          if (name === 'archive' && !confirm(`Archive ${profile.name}? Current and queued work will stop; history and Apps remain.`)) return;
          saving = true; sheet.setBusy(true); button.disabled = true; save.disabled = true;
          void post(`/agents/${profile.id}/${name === 'retry' ? 'retry' : 'lifecycle'}`, name === 'retry' ? {} : { action: name })
            .then(async () => { sheet.dialog.close(); await refresh(); })
            .catch(error => { sheet.status.textContent = error.message; })
            .finally(() => { saving = false; sheet.setBusy(false); button.disabled = false; save.disabled = !canEdit; });
        });
        if (name === 'archive' || name === 'delete') button.classList.add('danger'); advanced.append(button);
      }
    }
    sheet.show();
  }
  function filter(): void {
    const query = search.value.trim().toLowerCase();
    list.querySelectorAll<HTMLElement>('.manager-row').forEach(row => { row.hidden = !row.dataset.search?.includes(query); });
    const empty = list.querySelector<HTMLElement>('.ui-empty');
    if (empty) empty.hidden = !!list.querySelector('.manager-row:not([hidden])');
  }
  search.oninput = filter;
  let refreshing = false;
  async function refresh(): Promise<void> {
    if (refreshing) return; refreshing = true;
    try {
      directory = await api<Directory>('/agents'); create.disabled = !directory.creation_available;
      status.textContent = '';
      const next = JSON.stringify(directory); if (next === fingerprint) return; fingerprint = next;
      list.replaceChildren();
      for (const { profile } of directory.agents) {
        profile.name = displayAgentName(profile);
        const row = element('article', 'ui-row manager-row'); row.dataset.search = `${profile.name} ${profile.role}`.toLowerCase();
        const avatar = element('span', 'ui-avatar', profile.name.slice(0, 2).toUpperCase()); avatar.setAttribute('aria-hidden', 'true');
        const detail = element('div', 'ui-row-title'); detail.append(element('strong', '', profile.name), element('small', 'manager-description', profile.role));
        row.append(avatar, detail, element('span', 'ui-badge', profile.state));
        if (profile.state === 'ready') {
          const chat = element('a', 'ui-button', 'Chat'); chat.href = workspaceUrl(`/?agent=${profile.id}`); chat.setAttribute('aria-label', `Private chat with ${profile.name}`); row.append(chat);
        }
        row.append(uiButton('Manage', () => open(profile))); list.append(row);
      }
      list.append(element('p', 'ui-empty', directory.agents.length ? 'No agents match your search.' : 'Create an agent to help with your work.')); filter();
      status.textContent = '';
    } catch (error) { status.textContent = (error as Error).message; }
    finally { refreshing = false; }
  }
  await refresh(); view.interval(() => { if (!document.hidden && !editing) void refresh(); }, 5000);
}
