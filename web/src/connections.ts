import { api, post } from './types';
import { element } from './view';
import './connections.css';

type Connection = { id: string; toolkit: string; label: string; status: string };
type Grant = { connection_id: string; tool: string; version: string };
type State = { configured: boolean; connections: Connection[]; grants: Grant[] };
type Tool = { slug: string; name: string; description: string; version: string };

export function installConnections(container: HTMLElement, agentMode = false): void {
  container.classList.add('connections');
  container.innerHTML = `<h3>${agentMode ? 'Assigned connections' : 'Business connections'}</h3>
    <p>${agentMode ? 'Grant only the operations this agent needs. Every execution still requires an exact approval.' : 'Connect Google accounts through Composio. Assign operations separately in each agent’s Details.'}</p>
    <button class="files-button" data-refresh>Refresh connections</button><p data-status role="status"></p><div data-list></div><div data-editor></div>`;
  const status = container.querySelector<HTMLElement>('[data-status]')!;
  const list = container.querySelector<HTMLElement>('[data-list]')!;
  const editor = container.querySelector<HTMLElement>('[data-editor]')!;
  let state: State | undefined;
  let busy = false;
  function controls(): void { container.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = busy; }); }
  async function perform(action: () => Promise<void>): Promise<void> {
    if (busy) return;
    busy = true; controls(); status.textContent = '';
    try { await action(); }
    catch (e) { status.textContent = `${(e as Error).message} Inspect the saved state before retrying.`; }
    finally { busy = false; controls(); }
  }
  async function refresh(): Promise<void> {
    try {
      state = await api<State>('/connections');
      status.textContent = state.configured ? 'Connections loaded.' : 'Configure COMPOSIO_API_KEY in local .env and restart Ayati to connect accounts.';
      draw();
    } catch (e) { state = undefined; list.replaceChildren(); editor.replaceChildren(); status.textContent = (e as Error).message; }
  }
  function button(label: string, action: () => Promise<void>): HTMLButtonElement {
    const b = element('button', 'files-button', label); b.onclick = () => void perform(action); return b;
  }
  async function chooseTools(connection: Connection): Promise<void> {
    editor.replaceChildren(element('h3', '', `Operations for ${connection.label}`));
    const form = document.createElement('form');
    const search = document.createElement('input'); search.type = 'search'; search.placeholder = 'Search operations'; search.maxLength = 100; search.setAttribute('aria-label', 'Search connector operations');
    const submit = element('button', 'files-button', 'Search'); submit.type = 'submit';
    const results = element('div'); form.append(search, submit); editor.append(form, results);
    async function load(): Promise<void> {
      const data = await api<{ tools: Tool[]; has_more: boolean }>(`/connections/${connection.id}/tools?query=${encodeURIComponent(search.value)}`);
      results.replaceChildren();
      if (data.has_more) results.append(element('p', '', 'Showing the first 30 matches. Refine the search to find other operations.'));
      if (!data.tools.length) results.append(element('p', '', 'No matching operations.'));
      for (const tool of data.tools) {
        const row = element('article', 'connection-card');
        const enabled = state?.grants.some(g => g.connection_id === connection.id && g.tool === tool.slug) ?? false;
        row.append(element('strong', '', tool.name || tool.slug), element('p', '', tool.description), element('small', '', `${tool.slug} · ${tool.version}`));
        const grant = button(enabled ? 'Remove grant' : 'Grant to this agent', async () => {
          await post(`/connections/${connection.id}/grants`, { tool: tool.slug, version: tool.version, enabled: !enabled });
          await refresh(); await chooseTools(connection);
        });
        row.append(grant); results.append(row);
      }
    }
    form.onsubmit = e => { e.preventDefault(); void perform(load); };
    await load();
  }
  async function connect(): Promise<void> {
    const data = await api<{ configs: { id: string; name: string; toolkit: string }[] }>('/connections/configs');
    editor.replaceChildren();
    if (!data.configs.length) { editor.append(element('p', '', 'Create a Gmail, Google Drive or Google Calendar auth configuration in your Composio project, then refresh.')); return; }
    const form = document.createElement('form'); form.className = 'connection-form';
    const config = document.createElement('select'); config.setAttribute('aria-label', 'Service');
    for (const item of data.configs) { const option = document.createElement('option'); option.value = item.id; option.textContent = `${item.toolkit} · ${item.name}`; config.append(option); }
    const label = document.createElement('input'); label.placeholder = 'Account label, such as Work Gmail'; label.required = true; label.maxLength = 100; label.setAttribute('aria-label', 'Account label');
    const submit = element('button', 'files-button', 'Create sign-in link'); submit.type = 'submit';
    form.append(config, label, submit); editor.append(form);
    form.onsubmit = event => {
      event.preventDefault();
      void perform(async () => {
        const selected = data.configs.find(c => c.id === config.value)!;
        const linked = await post<{ id: string; redirect_url: string }>('/connections', { toolkit: selected.toolkit, auth_config_id: selected.id, label: label.value });
        const link = element('a', 'files-button', 'Continue to Google sign-in'); link.href = linked.redirect_url; link.target = '_blank'; link.rel = 'noopener noreferrer';
        form.replaceChildren(link, element('p', '', 'After completing sign-in, use Refresh account on the saved connection. This sign-in link is private; do not share it.'));
        await refresh();
      });
    };
  }
  function draw(): void {
    list.replaceChildren();
    if (!state) return;
    if (!state.connections.length) list.append(element('p', '', 'No connected business accounts.'));
    if (!agentMode && state.configured) list.append(button('Connect Google account', connect));
    for (const connection of state.connections) {
      const card = element('article', 'connection-card');
      card.append(element('strong', '', connection.label), element('p', '', `${connection.toolkit} · ${connection.status}`));
      if (connection.status !== 'REVOKED' && connection.status !== 'LINK_UNCERTAIN') {
        card.append(button('Refresh account', async () => { await post(`/connections/${connection.id}/refresh`); await refresh(); }));
        if (agentMode && connection.status === 'ACTIVE') card.append(button('Choose operations', () => chooseTools(connection)));
        if (!agentMode) card.append(button('Revoke workspace access', async () => { await post(`/connections/${connection.id}/revoke`); editor.replaceChildren(); await refresh(); }));
      }
      if (connection.status === 'LINK_UNCERTAIN') card.append(element('p', '', 'Sign-in link creation was not acknowledged. Inspect connections in Composio before starting another flow.'));
      const grants = state.grants.filter(g => g.connection_id === connection.id);
      if (agentMode) {
        card.append(element('p', '', `${grants.length} operations assigned to this agent`));
        for (const grant of grants) card.append(button(`Remove ${grant.tool}`, async () => {
          await post(`/connections/${connection.id}/grants`, { ...grant, enabled: false, connection_id: undefined }); await refresh();
        }));
      }
      list.append(card);
    }
    if (!agentMode) list.append(element('p', '', 'Workspace revocation blocks every agent locally and removes its grants. It does not revoke Google consent or delete the connection in Composio. An operation already sent may still finish.'));
  }
  container.querySelector<HTMLButtonElement>('[data-refresh]')!.onclick = () => void perform(refresh);
  if (agentMode) container.closest('dialog')?.addEventListener('workpanelopen', () => { void perform(refresh); });
  else void perform(refresh);
}
