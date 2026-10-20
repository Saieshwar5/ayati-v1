import { api, post } from './types';
import { canManageResources, workspaceUrl } from './workspace-context';
import { element } from './view';
import type { Profile } from './agents';
import './agents.css';
import './agent-manager.css';
type Directory = { agents: { profile: Profile; activity: { pending_tasks: number } }[]; creation_available: boolean; default_model: string; memory_available: boolean; browser_available: boolean };

export async function installAgentManager(): Promise<void> {
  const root = document.querySelector<HTMLDivElement>('#app')!;
  root.className = 'workspace-page'; document.title = 'Agents · Relay';
  root.append(element('h1', '', 'Agents'), element('p', 'page-subtitle', 'Create and configure your team’s agents. Conversations live in Chat.'));
  if (!canManageResources()) { root.append(element('p', '', 'Your workspace owner manages agent configuration. Open Chat to talk with agents shared with you.')); return; }
  const status = element('p'); status.setAttribute('role', 'status');
  const create = element('button', 'files-button', 'Create agent');
  const list = element('div', 'manager-grid'); root.append(create, status, list);
  const dialog = element('dialog', 'agent-setup'); dialog.setAttribute('aria-label', 'Agent configuration');
  dialog.innerHTML = `<form><div class="panel-header"><h2>Agent configuration</h2><button type="button" aria-label="Close">×</button></div>
    <label>Name<input name="name" required maxlength="80"></label>
    <label>Responsibilities<textarea name="role" required maxlength="4000"></textarea></label>
    <label>Model<input name="model" required maxlength="200"></label>
    <label><input type="checkbox" name="memory"> Sourced memory</label>
    <label><input type="checkbox" name="browser"> Browser</label>
    <p>Tools retain their existing approval checks. Changing responsibilities or tools starts fresh model context; saved history remains.</p>
    <p role="status"></p><button type="submit" class="files-button">Save</button></form>`;
  document.body.append(dialog);
  const form = dialog.querySelector('form')!;
  const field = (name: string) => form.elements.namedItem(name) as HTMLInputElement;
  let directory: Directory; let editing: Profile | undefined; let requestId = ''; let saving = false;
  dialog.querySelector('button')!.onclick = () => { if (!saving) dialog.close(); };
  dialog.addEventListener('cancel', event => { if (saving) event.preventDefault(); });
  function open(profile?: Profile): void {
    editing = profile; requestId = crypto.randomUUID(); form.reset();
    field('name').value = profile?.name ?? ''; field('role').value = profile?.role ?? '';
    field('model').value = profile?.model ?? directory.default_model;
    for (const name of ['memory', 'browser'] as const) {
      field(name).checked = profile?.[name] ?? false;
      field(name).disabled = name === 'browser' ? !(profile?.browser_available ?? directory.browser_available) && !profile?.browser : !directory.memory_available && !profile?.memory;
    }
    form.querySelector('[role=status]')!.textContent = ''; dialog.showModal(); field('name').focus();
  }
  create.onclick = () => open();
  form.onsubmit = async event => {
    event.preventDefault(); if (saving) return; saving = true;
    const submit = form.querySelector<HTMLButtonElement>('[type=submit]')!; submit.disabled = true;
    const input = { name: field('name').value, role: field('role').value, model: field('model').value, memory: field('memory').checked, browser: field('browser').checked };
    try {
      await post(editing ? `/agents/${editing.id}/profile` : '/agents', editing ? { ...input, expected_revision: editing.revision } : { ...input, request_id: requestId });
      dialog.close(); await refresh();
    } catch (error) { form.querySelector('[role=status]')!.textContent = (error as Error).message; }
    finally { saving = false; submit.disabled = false; }
  };
  async function action(profile: Profile, name: string): Promise<void> {
    if (name === 'delete' && !confirm(`Remove ${profile.name} from the agent directory? Saved files, chat history and Apps are retained.`)) return;
    if (name === 'archive' && !confirm(`Archive ${profile.name}? Current and queued work will stop; history and Apps remain.`)) return;
    try { await post(`/agents/${profile.id}/${name === 'retry' ? 'retry' : 'lifecycle'}`, name === 'retry' ? {} : { action: name }); await refresh(); }
    catch (error) { status.textContent = (error as Error).message; }
  }
  let refreshing = false;
  async function refresh(): Promise<void> {
    if (refreshing) return; refreshing = true;
    try {
      directory = await api<Directory>('/agents'); create.disabled = !directory.creation_available;
      list.replaceChildren();
      for (const { profile } of directory.agents) {
        if (profile.id === 'local-agent' && profile.name === 'Ayati') profile.name = 'Relay';
        const card = element('section', 'manager-card');
        card.append(element('h2', '', profile.name), element('p', 'page-subtitle', profile.role), element('p', '', profile.state));
        const controls = element('div', 'manager-actions');
        if (profile.state === 'ready') {
          const chat = element('a', 'files-button', 'Open private chat'); chat.href = workspaceUrl(`/?agent=${profile.id}`); controls.append(chat);
          const edit = element('button', 'files-button', 'Configure'); edit.disabled = !profile.editable; edit.onclick = () => open(profile); controls.append(edit);
        }
        const actions = profile.id === 'local-agent' ? [] : profile.state === 'archived' ? ['restore', 'delete'] : profile.state === 'failed' ? ['retry', 'archive'] : profile.state === 'ready' || profile.state === 'archiving' ? ['archive'] : [];
        for (const name of actions) { const button = element('button', 'text-button', name[0].toUpperCase() + name.slice(1)); button.onclick = () => { button.disabled = true; void action(profile, name); }; controls.append(button); }
        card.append(controls); list.append(card);
      }
    } catch (error) { status.textContent = (error as Error).message; }
    finally { refreshing = false; }
  }
  await refresh(); window.setInterval(() => { if (!document.hidden && !dialog.open) void refresh(); }, 5000);
}
