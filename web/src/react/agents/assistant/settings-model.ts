import { identityRequest, workspaceId } from '../../../workspace-context';

/** A member's assistant's regular checks and whether it learns (schemas 129 and 137).
 * These belong to the signed-in member: nobody else, admins included, reads them. */
export type AssistantSettings = { checks: boolean; every_minutes: number; checklist: string;
  /** Learn from their edits to drafts and from looking back at their chat. */
  learn?: boolean; available?: boolean;
  /** The regular check's task (schema 149): its next run, and paused when stopped on its task page. */
  task?: { id: string; next_ms: number | null; next_text: string | null; paused: boolean } | null };
const base = `/workspaces/${workspaceId}/assistant`;
export const loadAssistantSettings = (signal?: AbortSignal) => identityRequest<AssistantSettings>(`${base}/settings`, undefined, signal);
/** Saves the named fields and keeps the others, so each part of the page saves its own. */
export const saveAssistantSettings = (fields: Partial<Omit<AssistantSettings, 'available'>>) => identityRequest<{ ok: true }>(`${base}/settings`, fields);

/** One thing the assistant remembers about its person or their work (schema 138). */
export type MemoryKind = 'about' | 'style' | 'ways' | 'people' | 'work';
export type Memory = { id: string; kind: MemoryKind; text: string;
  /** Written by the person, or by the assistant while working with them or looking back. */
  source: 'you' | 'chat' | 'review'; created_ms: number; ended_ms: number | null };
export type Memories = { memories: Memory[]; /** Lines the assistant ended; its person can bring them back. */ ended: Memory[]; most: number; available?: boolean };
export const kinds: [MemoryKind, string][] = [['about', 'About you'], ['style', 'How you write'], ['ways', 'How you want things done'], ['people', 'People you work with'], ['work', 'Your work']];
export const loadMemories = () => identityRequest<Memories>(`${base}/memories`);
/** Adds a line, or changes one by its id. The same id saves once. */
export const saveMemory = (line: Pick<Memory, 'id' | 'kind' | 'text'>) => identityRequest<{ ok: true }>(`${base}/memories`, line);
/** `remove` deletes a line for good; `restore` brings back one the assistant ended. */
export const changeMemory = (id: string, change: 'remove' | 'restore') => identityRequest<{ ok: true }>(`${base}/memories/${encodeURIComponent(id)}/${change}`, {});
