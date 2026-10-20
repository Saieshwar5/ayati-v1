import { workspaceUrl } from './workspace-context';
import './agents.css';
import { api, selectedAgent } from './types';
import { element } from './view';

export type Profile = { browser_available: boolean; revision: number; editable: boolean; id: string; name: string; role: string; model: string; memory: boolean; browser: boolean; state: string; detail: string };
type Workspace = { agents: { profile: Profile; activity: { pending_approvals: number; pending_tasks: number; latest?: { text: string; status: string; created_at: string } } }[]; creation_available: boolean; memory_available: boolean; browser_available: boolean; default_model: string; limit: number };

export function installAgents(updateDetails: (profile: Profile) => void): void {
  const main = document.querySelector<HTMLElement>('.main')!;
  const panel = element('aside', 'agents-list');
  panel.setAttribute('aria-label', 'Agents'); panel.id = 'agents-sidebar';
  document.querySelector('#agent-sidebar-toggle')!.setAttribute('aria-controls', panel.id);
  panel.innerHTML = `<div class="agents-heading"><h2>Chat</h2><button id="new-agent" class="files-button" aria-label="New agent" title="New agent">＋</button></div>
    <label class="agent-search-label">Search agents<input id="agent-search" type="search" placeholder="Search agents…"></label><div id="agent-cards"></div><p id="agents-status" role="status"></p>`;
  main.prepend(panel);
  const all = element('a', 'files-button', 'People & group chats'); all.href = workspaceUrl('/?view=chat'); panel.prepend(all);
  const chooser = document.createElement('dialog'); chooser.className = 'agent-chooser';
  chooser.setAttribute('aria-label', 'Choose agent');
  const closeChooser = element('button', 'files-button', 'Back to conversation');
  chooser.append(closeChooser); document.body.append(chooser);
  const toggle = document.querySelector<HTMLButtonElement>('#agent-list-toggle')!;
  toggle.setAttribute('aria-haspopup', 'dialog');
  toggle.onclick = () => { chooser.append(panel); chooser.showModal(); panel.querySelector<HTMLInputElement>('#agent-search')!.focus(); };
  closeChooser.onclick = () => chooser.close();
  chooser.addEventListener('keydown', event => {
    if (event.key === 'Escape') { event.preventDefault(); chooser.close(); }
  });
  chooser.addEventListener('close', () => { main.prepend(panel); toggle.focus(); });
  matchMedia('(max-width: 760px)').addEventListener('change', event => { if (!event.matches && chooser.open) chooser.close(); });
  const ownerApi = <T>(path: string): Promise<T> => api<T>(path, { headers: { 'X-Ayati-Agent': 'local-agent' } });
  const create = panel.querySelector<HTMLButtonElement>('#new-agent')!;
  create.textContent = 'Manage'; create.setAttribute('aria-label', 'Manage agents'); create.title = 'Manage agents';
  create.onclick = () => location.assign(workspaceUrl('/?view=agents'));
  const status = panel.querySelector<HTMLElement>('#agents-status')!;
  const cards = panel.querySelector<HTMLElement>('#agent-cards')!;
  const search = panel.querySelector<HTMLInputElement>('#agent-search')!;
  const empty = element('p', 'agent-detail', 'No matching agents.'); empty.hidden = true; panel.append(empty);
  function filter(): void {
    const query = search.value.trim().toLocaleLowerCase();
    const rows = Array.from(cards.children) as HTMLElement[];
    for (const row of rows) row.hidden = !row.dataset.search?.includes(query);
    empty.hidden = !rows.length || rows.some(row => !row.hidden);
  }
  search.oninput = filter;
  let refreshing = false;
  let workspace: Workspace | undefined;
  let fingerprint = '';


  async function refresh(): Promise<void> {
    if (refreshing) return;
    refreshing = true;
    try {
      workspace = await ownerApi<Workspace>('/agents');
      // Present the built-in guide under the product name without changing saved profiles.
      workspace.agents = workspace.agents.filter(a => a.profile.state !== 'archived').map(agent => agent.profile.id === 'local-agent' && agent.profile.name === 'Ayati'
        ? { ...agent, profile: { ...agent.profile, name: 'Relay' } } : agent);

      status.textContent = !workspace.creation_available ? 'Creating agents requires local VM setup.'
        : workspace.agents.length >= workspace.limit ? `Local limit: ${workspace.limit} agents.` : '';
      const current = workspace.agents.find(a => a.profile.id === selectedAgent)?.profile;
      if (current) {
        updateDetails(current);
        document.querySelector('.identity strong')!.textContent = current.name;
        document.querySelector('.identity .avatar')!.textContent = current.name.split(/\s+/).map(part => part[0]).slice(0, 2).join('').toUpperCase();
        document.querySelector('#input')!.setAttribute('aria-label', `Message ${current.name}`);
        document.title = current.name === 'Relay' ? 'Chat · Relay' : `${current.name} · Relay`;
        const input = document.querySelector<HTMLTextAreaElement>('#input')!;
        input.placeholder = `Message ${current.name}…`;
        document.querySelector('.chat')!.setAttribute('aria-label', `Conversation with ${current.name}`);
      }
      const next = JSON.stringify(workspace.agents);
      if (next === fingerprint) return;
      fingerprint = next;
      cards.replaceChildren(...workspace.agents.map(({ profile, activity }) => {
        const card = element('div', 'agent-card');
        card.dataset.state = profile.state !== 'ready' ? profile.state
          : activity.pending_approvals ? 'waiting' : activity.pending_tasks ? 'working' : 'ready';
        card.dataset.search = `${profile.name} ${profile.role}`.toLocaleLowerCase();
        if (profile.id === selectedAgent) card.classList.add('selected');
        const avatar = element('span', 'agent-avatar', profile.name.split(/\s+/).map(part => part[0]).slice(0, 2).join('').toUpperCase()); avatar.setAttribute('aria-hidden', 'true');
        card.append(avatar);
        const link = element('a', 'agent-link', profile.name);
        link.href = workspaceUrl(`/?agent=${encodeURIComponent(profile.id)}`);
        if (profile.state !== 'ready') { link.removeAttribute('href'); link.setAttribute('aria-disabled', 'true'); }
        if (profile.id === selectedAgent) link.setAttribute('aria-current', 'page');
        card.append(link);
        card.title = profile.state === 'ready' ? profile.name : profile.detail;
        if (activity.pending_approvals) {
          const badge = element('span', 'agent-attention', String(activity.pending_approvals));
          badge.setAttribute('aria-label', `${activity.pending_approvals} approvals need attention`); card.append(badge);
        }
        return card;
      }));
      filter();
    } catch { status.textContent = 'Agent list unavailable. Reconnecting…'; }
    finally { refreshing = false; }
  }
  void refresh();
  window.setInterval(() => { if (!document.hidden) void refresh(); }, 2500);
}
