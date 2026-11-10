import { identityRequest } from '../../../workspace-context';
import { base } from '../model';

/** An agent's email address (runtime/email-channel.md). Off until its owner or an
 * admin switches it on; Cuckoo makes the address on its own mail domain. */
export type EmailStatus = { offered: boolean; state: 'off' | 'active' | 'paused'; address: string | null; can_manage: boolean };

const path = (agent: string) => `${base}/${encodeURIComponent(agent)}/email`;
export const loadEmail = (agent: string) => identityRequest<EmailStatus>(path(agent));
export const activateEmail = (agent: string) => identityRequest<EmailStatus>(`${path(agent)}/activate`, {});
export const deactivateEmail = (agent: string) => identityRequest<EmailStatus>(`${path(agent)}/deactivate`, {});
