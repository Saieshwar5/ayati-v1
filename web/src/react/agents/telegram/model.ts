import { identityRequest, workspaceId } from '../../../workspace-context';
import { base } from '../model';

/** An agent's own Telegram bot (runtime/telegram-channel.md). Off until its owner
 * or an admin switches it on; the workspace's Telegram owner then creates the
 * bot with one tap in Telegram. */
export type TelegramStatus = {
  offered: boolean; connected: boolean; state: 'off' | 'requested' | 'active' | 'paused';
  username: string | null; link: string | null; create_link: string | null;
  is_owner: boolean; can_manage: boolean; owner_asked?: boolean;
};
/** The workspace's Telegram: connected once by an admin, its Telegram owner. */
export type WorkspaceTelegram = { offered: boolean; connected: boolean; owner: string | null; is_owner: boolean; can_manage: boolean };
/** The signed-in person's own Telegram account. */
export type TelegramAccount = { offered?: boolean; linked: boolean; username: string | null };
export type StartLink = { link: string; expires_in: number };

const path = (agent: string) => `${base}/${encodeURIComponent(agent)}/telegram`;
export const loadTelegram = (agent: string) => identityRequest<TelegramStatus>(path(agent));
export const activateTelegram = (agent: string) => identityRequest<TelegramStatus>(`${path(agent)}/activate`, {});
export const deactivateTelegram = (agent: string) => identityRequest<TelegramStatus>(`${path(agent)}/deactivate`, {});
export const loadWorkspaceTelegram = () => identityRequest<WorkspaceTelegram>(`/workspaces/${workspaceId}/telegram`);
export const connectWorkspaceTelegram = () => identityRequest<StartLink>(`/workspaces/${workspaceId}/telegram/connect`, {});
export const loadTelegramAccount = () => identityRequest<TelegramAccount>('/telegram');
export const linkTelegram = () => identityRequest<StartLink>('/telegram/link', {});
export const unlinkTelegram = () => identityRequest<TelegramAccount>('/telegram/unlink', {});

/** Opens a Telegram link in a new tab. Opened before the request finishes and
 * pointed at the link after, so the browser doesn't treat it as a pop-up. */
export async function openTelegram(link: () => Promise<string>): Promise<boolean> {
  const tab = window.open('', '_blank');
  try {
    const url = await link();
    if (tab) tab.location.href = url; else window.location.assign(url);
    return true;
  } catch (error) {
    tab?.close();
    throw error;
  }
}
