import { currentView } from './view-lifetime';
import { canManageResources } from './workspace-context';

import { element } from './view';
import { renderPanel, type PanelDefinition } from './panel-renderer';
import { installPanelRequest } from './panel-request';
import './home-panels.css';
import { chooseAgent, conversationUrl } from './agent-request';
import { arrangement, loadLayout, panelTemplate } from './panel-layout';
import { businessDate } from './home-clock';

type Panel = { id: string; agent_id: string; revision: number; active_revision: number | null; updates_enabled: boolean; draft: PanelDefinition | null; active: { definition: PanelDefinition; updated_at: string } | null; state: string; source_executions: string[]; updated_at: string; maintenance?: { schedule: { state: string; next_run_ms: number | null } | null; latest_run: { task: { status: string; detail: string } } | null }[] };

export function installHomePanels(container: HTMLElement): void {
  const view = currentView();
  const { api, post } = view;
  container.innerHTML = `<div class="home-section-heading"><h2>Your panels</h2><div class="panel-heading-actions"><button class="text-button" data-arrange hidden>Done</button><button class="text-button" data-request-panel hidden>＋ Add panel</button></div></div>
    <p data-panel-status role="status"></p><div class="home-panels" data-panel-list></div><div data-panel-request hidden></div>`;
  const status = container.querySelector<HTMLElement>('[data-panel-status]')!;
  const list = container.querySelector<HTMLElement>('[data-panel-list]')!;
  installPanelRequest(container);
  container.querySelector<HTMLElement>('.panel-heading-actions')!.hidden = !canManageResources();
  let busy = false, loading = false, fingerprint = '', arranging = false;
  let current: Panel[] = [];
  const layout = loadLayout();
  container.querySelector<HTMLButtonElement>('[data-arrange]')!.onclick = event => {
    arranging = !arranging; list.classList.toggle('is-arranging', arranging); (event.currentTarget as HTMLElement).setAttribute('aria-pressed', String(arranging)); (event.currentTarget as HTMLElement).hidden = !arranging; draw(current);
  };
  async function act(action: () => Promise<unknown>): Promise<void> {
    if (busy) return; busy = true;
    list.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; });
    try { await action(); fingerprint = ''; }
    catch (e) { status.textContent = `${(e as Error).message} Refresh Home before retrying.`; }
    finally { busy = false; await refresh(); }
  }
  function draw(panels: Panel[]): void {
    const expanded = new Set([...list.querySelectorAll<HTMLDetailsElement>('details[open][data-settings]')].map(d => d.dataset.settings));
    current = panels;
    list.classList.toggle('is-empty', panels.length === 0);
    list.replaceChildren();
    if (!panels.length && !canManageResources()) {
      const welcome = element('div', 'home-welcome');
      welcome.append(element('h3', '', 'Welcome to your workspace'), element('p', '', 'Your team is in Chat and Users. Panels shared with you will appear here.'));
      list.append(welcome); return;
    }
    if (!panels.length) {
      const welcome = element('div', 'home-welcome');
      welcome.append(element('h3', '', 'A home for what matters.'), element('p', '', 'Keep the information you use every day close by.'));
      const add = element('button', 'ui-button primary', 'Add your first panel');
      add.onclick = () => chooseAgent('panel');
      welcome.append(add); list.append(welcome);
    }
    const ordered = [...panels].sort((a, b) => {
      const rank = (id: string): number => { const index = layout.order.indexOf(id); return index < 0 ? 10000 : index; };
      return rank(a.id) - rank(b.id);
    });
    for (const panel of ordered) {
      const card = element('article', 'home-panel-card'); card.dataset.panel = panel.id;
      const template = panelTemplate(panel.active?.definition ?? panel.draft);
      card.dataset.template = template;
      card.classList.toggle('panel-wide', template !== 'metric' && layout.wide.includes(panel.id));
      if (arranging) arrangement(card, panel.id, ordered.map(p => p.id), layout, template, () => draw(current));
      if (panel.active) card.append(renderPanel(panel.active.definition));
      else if (panel.draft) card.append(renderPanel(panel.draft));
      else card.append(element('h3', '', 'Panel source unavailable'), element('p', '', 'Saved content is hidden because its account or operation grant is no longer available.'));
      const date = panel.active?.updated_at ?? panel.updated_at;
      const footer = element('div', 'panel-footer');
      footer.append(element('small', '', `Updated ${businessDate(date.replace(' ', 'T') + (date.endsWith('Z') ? '' : 'Z'))}`));
      const agent = element('a', 'text-button', 'Ask its agent ↗'); agent.href = conversationUrl(panel.agent_id, 'edit-panel', panel.id); if (canManageResources()) footer.append(agent); card.append(footer);
      if (!canManageResources()) { list.append(card); continue; }
      const settings = document.createElement('details'); settings.className = 'panel-settings'; settings.dataset.settings = panel.id; settings.open = expanded.has(panel.id);
      settings.append(element('summary', '', 'Updates and details'));
      if (template === 'metric') settings.append(element('p', 'panel-purpose', (panel.active?.definition ?? panel.draft)?.purpose ?? ''));
      if (panel.state === 'awaiting_review' && panel.draft) {
        const proposal = document.createElement('details'); proposal.open = !panel.active;
        proposal.append(element('summary', '', `Review revision ${panel.revision}`));
        if (panel.active) proposal.append(renderPanel(panel.draft));
        proposal.append(element('p', '', panel.source_executions.length ? `${panel.source_executions.length} successful connector executions cited. A source receipt does not independently prove the agent’s interpretation.` : 'Agent findings without connector receipts. Review the basis and assumptions before relying on them.'));
        const label = document.createElement('label'), updates = document.createElement('input'); updates.type = 'checkbox'; updates.checked = panel.updates_enabled;
        label.append(updates, document.createTextNode(' Allow this agent to refresh this panel using these same sources'));
        const keep = element('button', 'files-button', 'Keep on Home'); keep.onclick = () => void act(() => post(`/panels/${panel.id}/activate`, { revision: panel.revision, updates_enabled: updates.checked }));
        proposal.append(label, keep); card.append(proposal);
      }
      if (panel.state === 'active') {
        const updates = element('button', 'text-button', panel.updates_enabled ? 'Pause panel updates' : 'Allow panel updates');
        updates.onclick = () => void act(() => post(`/panels/${panel.id}/updates`, { revision: panel.revision, updates_enabled: !panel.updates_enabled })); settings.append(updates);
        settings.append(element('p', 'panel-update-note', panel.maintenance?.length ? 'Bound schedules pause when updates are paused or this panel is removed. Active work retains its Stop control.' : 'Ask the maintaining agent to schedule updates for this panel. Update permission alone does not create a schedule.'));
      }
      for (const maintenance of panel.maintenance ?? []) settings.append(element('p', 'panel-update-note', `Maintenance: ${maintenance.schedule?.state ?? 'unavailable'}${maintenance.latest_run ? ` · last run ${maintenance.latest_run.task.status}: ${maintenance.latest_run.task.detail}` : ' · no runs yet'}${maintenance.schedule?.next_run_ms ? ` · next due ${businessDate(maintenance.schedule.next_run_ms)}` : ''}`));
      const remove = element('button', 'text-button', 'Remove panel'); remove.onclick = () => void act(() => post(`/panels/${panel.id}/remove`, { revision: panel.revision })); settings.append(remove); card.append(settings); list.append(card);
    }
  }
  async function refresh(): Promise<void> {
    if (loading || busy || !view.active) return; loading = true;
    try {
      const data = await api<{ panels: Panel[]; available: boolean }>('/panels');
      const next = JSON.stringify(data.panels); if (next !== fingerprint) { fingerprint = next; draw(data.panels); }
      status.textContent = data.available ? '' : 'Agent-created panels require Goose execution.';
      list.querySelectorAll<HTMLButtonElement>('button:not([data-no-enable])').forEach(b => { b.disabled = false; });
    } catch { status.textContent = 'Panel connection lost. Saved views may be stale; decisions are disabled until reconnected.'; list.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; }); }
    finally { loading = false; }
  }
  view.onWindow('business-clock-changed', () => draw(current));
  view.interval(() => { if (!document.hidden) void refresh(); }, 4000); view.onWindow('focus', () => { void refresh(); }); void refresh();
}
