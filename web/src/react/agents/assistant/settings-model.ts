import { identityRequest, workspaceId } from '../../../workspace-context';

/** What a member's assistant knows about them, and its regular checks (schema 129).
 * These belong to the signed-in member: nobody else, admins included, reads them. */
export type AssistantSettings = { about: string; style: string; checks: boolean; every_minutes: number; checklist: string;
  /** Learn how they write from what they change in a draft before sending it (schema 137). */
  learn?: boolean; available?: boolean };
const path = `/workspaces/${workspaceId}/assistant/settings`;
export const loadAssistantSettings = (signal?: AbortSignal) => identityRequest<AssistantSettings>(path, undefined, signal);
/** Saves the named fields and keeps the others, so each part of the page saves its own. */
export const saveAssistantSettings = (fields: Partial<Omit<AssistantSettings, 'available'>>) => identityRequest<{ ok: true }>(path, fields);
