import { api, post } from './types';
import { element } from './view';
import { renderPanel, type PanelDefinition } from './panel-renderer';
import { installPanelRequest } from './panel-request';
import './home-panels.css';

type Panel = { id: string; agent_id: string; revision: number; active_revision: number | null; updates_enabled: boolean; draft: PanelDefinition | null; active: { definition: PanelDefinition; updated_at: string } | null; state: string; source_executions: string[]; updated_at: string; maintenance?: { schedule: { state: string; next_run_ms: number | null } | null; latest_run: { task: { status: string; detail: string } } | null }[] };

export function installHomePanels(container: HTMLElement): void {
  container.innerHTML = `<div class="home-section-heading"><h2>Your panels</h2><button class="files-button" data-request-panel>Ask for a panel</button></div>
    <p data-panel-status role="status"></p><div class="home-panels" data-panel-list></div><div data-panel-request hidden></div>`;
  const status = container.querySelector<HTMLElement>('[data-panel-status]')!;
  const list = container.querySelector<HTMLElement>('[data-panel-list]')!;
  installPanelRequest(container);
  let busy = false, loading = false, fingerprint = '';
  async function act(action: () => Promise<unknown>): Promise<void> {
    if (busy) return; busy = true;
    list.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; });
    try { await action(); fingerprint = ''; }
    catch (e) { status.textContent = `${(e as Error).message} Refresh Home before retrying.`; }
    finally { busy = false; await refresh(); }
  }
  function draw(panels: Panel[]): void {
    list.replaceChildren();
    if (!panels.length) list.append(element('p', 'home-empty', 'Ask an agent to create a panel for information you want to keep visible.'));
    for (const panel of panels) {
      const card = element('article', 'home-panel-card'); card.dataset.panel = panel.id;
      if (panel.active) card.append(renderPanel(panel.active.definition));
      else if (panel.draft) card.append(renderPanel(panel.draft));
      else card.append(element('h3', '', 'Panel source unavailable'), element('p', '', 'Saved content is hidden because its account or operation grant is no longer available.'));
      const date = panel.active?.updated_at ?? panel.updated_at;
      card.append(element('small', '', `Saved snapshot · ${new Date(date.replace(' ', 'T') + 'Z').toLocaleString()}`));
      const agent = element('a', 'text-button', 'Open maintaining agent'); agent.href = `/?agent=${encodeURIComponent(panel.agent_id)}`; card.append(agent);
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
        updates.onclick = () => void act(() => post(`/panels/${panel.id}/updates`, { revision: panel.revision, updates_enabled: !panel.updates_enabled })); card.append(updates);
        card.append(element('p', 'panel-update-note', panel.maintenance?.length ? 'Bound schedules pause when updates are paused or this panel is removed. Active work retains its Stop control.' : 'Ask the maintaining agent to schedule updates for this panel. Update permission alone does not create a schedule.'));
      }
      for (const maintenance of panel.maintenance ?? []) card.append(element('p', 'panel-update-note', `Maintenance: ${maintenance.schedule?.state ?? 'unavailable'}${maintenance.latest_run ? ` · last run ${maintenance.latest_run.task.status}: ${maintenance.latest_run.task.detail}` : ' · no runs yet'}${maintenance.schedule?.next_run_ms ? ` · next due ${new Date(maintenance.schedule.next_run_ms).toLocaleString()}` : ''}`));
      const remove = element('button', 'text-button', 'Remove panel'); remove.onclick = () => void act(() => post(`/panels/${panel.id}/remove`, { revision: panel.revision })); card.append(remove); list.append(card);
    }
  }
  async function refresh(): Promise<void> {
    if (loading || busy || document.hidden) return; loading = true;
    try {
      const data = await api<{ panels: Panel[]; available: boolean }>('/panels');
      const next = JSON.stringify(data.panels); if (next !== fingerprint) { fingerprint = next; draw(data.panels); }
      status.textContent = data.available ? '' : 'Agent-created panels require Goose execution.';
      list.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = false; });
    } catch { status.textContent = 'Panel connection lost. Saved views may be stale; decisions are disabled until reconnected.'; list.querySelectorAll<HTMLButtonElement>('button').forEach(b => { b.disabled = true; }); }
    finally { loading = false; }
  }
  window.setInterval(() => { void refresh(); }, 4000); window.addEventListener('focus', () => { void refresh(); }); void refresh();
}
