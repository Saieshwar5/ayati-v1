import { identityRequest } from '../../../workspace-context';
import { base } from '../model';

/** An agent's email address (runtime/email-channel.md). Off until its owner or
 * an admin switches it on; the address is kept for the agent when switched off. */
export type EmailStatus = { offered: boolean; state: 'off' | 'active'; address: string | null; can_manage: boolean };

const path = (agent: string) => `${base}/${encodeURIComponent(agent)}/email`;
export const loadEmail = (agent: string) => identityRequest<EmailStatus>(path(agent));
export const activateEmail = (agent: string) => identityRequest<EmailStatus>(`${path(agent)}/activate`, {});
export const deactivateEmail = (agent: string) => identityRequest<EmailStatus>(`${path(agent)}/deactivate`, {});

/** A name as the server writes it in an address: lowercase letters and digits,
 * words joined by hyphens, at most 24 characters. Only for the preview; the
 * server makes the real address (`email_addresses::slug`). */
export function addressPart(name: string, fallback: string): string {
  let out = '';
  for (const c of name.toLowerCase()) {
    if (/[a-z0-9]/.test(c)) out += c;
    else if (/\p{L}|\p{N}/u.test(c)) continue;
    else if (out && !out.endsWith('-')) out += '-';
  }
  out = out.replace(/-+$/, '').slice(0, 24).replace(/-+$/, '');
  return out || fallback;
}

export const addressPreview = (agent: string, workspace: string, domain: string) =>
  `${addressPart(agent, 'agent')}.${addressPart(workspace, 'team')}@${domain}`;
