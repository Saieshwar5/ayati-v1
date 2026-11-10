import { identityRequest } from '../../../workspace-context';
import { base } from '../model';

/** An agent's WhatsApp number (runtime/whatsapp-channel.md). Off until its owner
 * or an admin switches it on; Cuckoo provides the number. */
export type WhatsappStatus = { offered: boolean; state: 'off' | 'active' | 'paused'; number: string | null; can_manage: boolean };
/** The signed-in person's own WhatsApp number, linked by sending a code from it. */
export type Mobile = { number: string | null; verified?: number; offered?: boolean };
export type LinkCode = { code: string; expires_in: number; send_to: string; link: string };

const path = (agent: string) => `${base}/${encodeURIComponent(agent)}/whatsapp`;
export const loadWhatsapp = (agent: string) => identityRequest<WhatsappStatus>(path(agent));
export const activateWhatsapp = (agent: string) => identityRequest<WhatsappStatus>(`${path(agent)}/activate`, {});
export const deactivateWhatsapp = (agent: string) => identityRequest<WhatsappStatus>(`${path(agent)}/deactivate`, {});
export const loadMobile = () => identityRequest<Mobile>('/mobile');
export const linkMobile = () => identityRequest<LinkCode>('/mobile/link', {});
export const unlinkMobile = () => identityRequest<Mobile>('/mobile/unlink', {});

/** `+14155550142` → `+1 415 555 0142`; other countries in groups of three or four. */
export function displayNumber(number: string): string {
  const digits = number.replace(/^\+/, '');
  if (digits.startsWith('1') && digits.length === 11) return `+1 ${digits.slice(1, 4)} ${digits.slice(4, 7)} ${digits.slice(7)}`;
  if (digits.startsWith('91') && digits.length === 12) return `+91 ${digits.slice(2, 7)} ${digits.slice(7)}`;
  return `+${digits.replace(/(\d{3})(?=\d{4})/g, '$1 ')}`;
}
