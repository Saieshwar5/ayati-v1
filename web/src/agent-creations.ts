import { api, selectedAgent } from './types';
import { element } from './view';

type Panel = { id: string; agent_id: string; state: string; active: { definition: { title: string } } | null; draft: { title: string } | null };
type App = { id: string; agent_id: string; name: string; active_revision: number | null };

// Durable creations remain discoverable even if a reply is interrupted or contains no link.
export function installAgentCreations(): void {
  const details = document.createElement('details'); details.className = 'agent-creations'; details.hidden = true;
  const summary = element('summary', '', 'Made with this agent'); const list = element('div'); details.append(summary, list);
  document.querySelector('.composer-area')!.before(details);
  let loading = false, fingerprint = '';
  async function refresh(): Promise<void> {
    if (loading || document.hidden) return; loading = true;
    try {
      const [panels, apps] = await Promise.all([api<{ panels: Panel[] }>('/panels'), api<{ apps: App[] }>('/apps')]);
      const views = panels.panels.filter(p => p.agent_id === selectedAgent && (p.active || p.draft));
      const tools = apps.apps.filter(a => a.agent_id === selectedAgent);
      const next = JSON.stringify([views, tools]); if (next === fingerprint) return; fingerprint = next;
      details.hidden = !views.length && !tools.length;
      summary.textContent = `Made with this agent · ${views.length + tools.length}`;
      list.replaceChildren();
      for (const panel of views) {
        const link = element('a', 'text-button', `${panel.active?.definition.title ?? panel.draft!.title} · ${panel.state === 'active' ? 'On Home' : 'Review on Home'} ↗`);
        link.href = '/?view=home'; link.target = window.self !== window.top ? '_top' : '_self'; list.append(link);
      }
      for (const app of tools) {
        const link = element('a', 'text-button', `${app.name} · ${app.active_revision === null ? 'Ready to try' : 'Open in Apps'} ↗`);
        link.href = '/?view=apps'; link.target = window.self !== window.top ? '_top' : '_self'; list.append(link);
      }
    } catch { /* Chat remains available when the creations service is unavailable. */ }
    finally { loading = false; }
  }
  void refresh(); window.setInterval(() => void refresh(), 5000);
}
