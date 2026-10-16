import './agents.css';
import { api, selectedAgent } from './types';
import { element } from './view';

type Profile = { id: string; name: string; role: string; model: string; memory: boolean; browser: boolean; state: string; detail: string };
type Workspace = { agents: { profile: Profile; activity: { pending_approvals: number; pending_tasks: number } }[]; creation_available: boolean; memory_available: boolean; browser_available: boolean; default_model: string; limit: number };

export function installAgents(): void {
  const main = document.querySelector<HTMLElement>('.main')!;
  const panel = element('aside', 'agents-list');
  panel.setAttribute('aria-label', 'Agents');
  panel.innerHTML = `<div class="agents-heading"><h2>Agents</h2><button id="new-agent" class="files-button">New agent</button></div>
    <p class="agents-hint">Independent roles. Separate conversations.</p><div id="agent-cards"></div><p id="agents-status" role="status"></p>`;
  main.prepend(panel);
  const dialog = document.createElement('dialog');
  dialog.className = 'agent-setup';
  dialog.setAttribute('aria-labelledby', 'setup-title');
  dialog.innerHTML = `<form id="agent-setup-form">
    <div class="panel-header"><h2 id="setup-title">Create an agent</h2><button type="button" id="setup-close" class="icon-button" aria-label="Close agent setup">×</button></div>
    <p>Give this agent a lasting responsibility. It gets its own conversation, memory and private environment.</p>
    <label>Name<input name="name" required maxlength="80" placeholder="e.g. Operations assistant"></label>
    <label>Role<textarea name="role" required maxlength="4000" placeholder="What should this agent help you accomplish?"></textarea></label>
    <label>Model<input name="model" required maxlength="200"></label>
    <p class="setup-help">Use a model supported by your configured AI provider.</p>
    <fieldset><legend>Tools</legend>
      <label><input name="memory" type="checkbox"> Sourced memory</label>
      <label><input name="browser" type="checkbox"> Browser</label>
      <p class="setup-help">Available tools need your approval for each action. Disabled options need local setup first.</p>
    </fieldset>
    <p>Preparation can take several minutes. You can continue using your other agents while it runs.</p>
    <p id="setup-error" role="alert"></p><button id="setup-submit" class="files-button">Create agent</button>
  </form>`;
  document.body.append(dialog);
  const form = dialog.querySelector<HTMLFormElement>('form')!;
  const create = panel.querySelector<HTMLButtonElement>('#new-agent')!;
  const submit = dialog.querySelector<HTMLButtonElement>('#setup-submit')!;
  const status = panel.querySelector<HTMLElement>('#agents-status')!;
  const error = dialog.querySelector<HTMLElement>('#setup-error')!;
  const cards = panel.querySelector<HTMLElement>('#agent-cards')!;
  const ownerApi = <T>(path: string, body?: unknown): Promise<T> => api<T>(path, {
    headers: { 'X-Ayati-Agent': 'local-agent', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) },
    ...(body === undefined ? {} : { method: 'POST', body: JSON.stringify(body) }),
  });
  let workspace: Workspace | undefined;
  let fingerprint = '';
  let busy = false;
  let requestId = crypto.randomUUID();
  create.disabled = true;

  async function refresh(): Promise<void> {
    try {
      workspace = await ownerApi<Workspace>('/agents');
      create.disabled = !workspace.creation_available || workspace.agents.length >= workspace.limit || workspace.agents.some(a => a.profile.state === 'preparing');
      status.textContent = !workspace.creation_available ? 'Creating agents requires local VM setup.'
        : workspace.agents.length >= workspace.limit ? `Local limit: ${workspace.limit} agents.` : '';
      const current = workspace.agents.find(a => a.profile.id === selectedAgent)?.profile;
      if (current) {
        document.querySelector('.identity strong')!.textContent = current.name;
        document.title = `${current.name} · Ayati`;
        const input = document.querySelector<HTMLTextAreaElement>('#input')!;
        input.placeholder = `Message ${current.name}…`;
        document.querySelector('.chat')!.setAttribute('aria-label', `Conversation with ${current.name}`);
      }
      const next = JSON.stringify(workspace.agents);
      if (next === fingerprint) return;
      fingerprint = next;
      cards.replaceChildren(...workspace.agents.map(({ profile, activity }) => {
        const card = element('div', 'agent-card');
        const link = element('a', 'agent-link', profile.name);
        link.href = `/?agent=${encodeURIComponent(profile.id)}`;
        if (profile.state !== 'ready') { link.removeAttribute('href'); link.setAttribute('aria-disabled', 'true'); }
        if (profile.id === selectedAgent) link.setAttribute('aria-current', 'page');
        const detail = element('p', 'agent-detail', profile.state === 'ready'
          ? activity.pending_approvals ? `${activity.pending_approvals} awaiting approval`
            : activity.pending_tasks ? `${activity.pending_tasks} active or queued` : 'Ready'
          : profile.detail);
        card.append(link, detail);
        if (profile.role) card.append(element('p', 'agent-role', profile.role));
        if (profile.state === 'failed') {
          const retry = element('button', 'text-button', 'Retry setup');
          retry.onclick = async () => {
            retry.disabled = true;
            try { await ownerApi(`/agents/${profile.id}/retry`, {}); await refresh(); }
            catch (e) { status.textContent = (e as Error).message; retry.disabled = false; }
          };
          card.append(retry);
        }
        return card;
      }));
    } catch { status.textContent = 'Agent list unavailable. Reconnecting…'; }
  }
  create.onclick = () => {
    if (!workspace) return;
    form.reset(); error.textContent = ''; requestId = crypto.randomUUID();
    (form.elements.namedItem('model') as HTMLInputElement).value = workspace.default_model;
    for (const name of ['memory', 'browser'] as const) {
      const field = form.elements.namedItem(name) as HTMLInputElement;
      field.disabled = !workspace[`${name}_available`]; field.checked = !field.disabled;
    }
    dialog.showModal(); (form.elements.namedItem('name') as HTMLInputElement).focus();
  };
  dialog.querySelector<HTMLButtonElement>('#setup-close')!.onclick = () => dialog.close();
  form.onsubmit = async event => {
    event.preventDefault();
    if (busy) return;
    busy = true; submit.disabled = true; error.textContent = '';
    const data = new FormData(form);
    try {
      await ownerApi('/agents', { request_id: requestId, name: data.get('name'), role: data.get('role'), model: data.get('model'), memory: data.has('memory'), browser: data.has('browser') });
      dialog.close(); await refresh();
    } catch (e) { error.textContent = (e as Error).message; }
    finally { busy = false; submit.disabled = false; }
  };
  void refresh();
  window.setInterval(() => { if (!document.hidden) void refresh(); }, 2500);
}
