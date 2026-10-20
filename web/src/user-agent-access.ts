import { identityRequest, workspaceId } from './workspace-context.ts';
import { element } from './view.ts';
import type { Directory } from './shared-chat';

export async function installUserAgentAccess(root: HTMLElement, user: string, status: HTMLElement, setBusy: (busy: boolean) => void): Promise<void> {
  const base = `/workspaces/${workspaceId}`;
  const data = await identityRequest<Directory>(`${base}/chat-directory`);
  root.replaceChildren(element('p', 'ui-help', 'Allow this person to ask agents for replies in shared Chat. Private history, configuration and business tools stay separate. Changes save immediately.'));
  if (data.role !== 'Owner') return;
  for (const agent of data.agents) {
    const label = element('label', 'ui-check');
    const checkbox = element('input'); checkbox.type = 'checkbox';
    checkbox.checked = data.grants.some(g => g.user === user && g.agent === agent.id);
    checkbox.onchange = async () => {
      checkbox.disabled = true; setBusy(true); status.textContent = 'Saving agent access…';
      try { await identityRequest(`${base}/chat-grants`, { user, agent: agent.id, allowed: checkbox.checked }); status.textContent = 'Agent access saved.'; }
      catch (error) { checkbox.checked = !checkbox.checked; status.textContent = (error as Error).message; }
      finally { checkbox.disabled = false; setBusy(false); }
    };
    label.append(checkbox, element('span', '', agent.name)); root.append(label);
  }
  if (!data.agents.length) root.append(element('p', 'ui-empty', 'No agents available to share.'));
}
