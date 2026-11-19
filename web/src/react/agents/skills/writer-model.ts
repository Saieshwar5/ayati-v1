import { identityRequest, workspaceId } from '../../../workspace-context';
import type { SkillDetail, SkillFile } from './model';

/** Skills written in chat (designs/agent-built-skills 2 and 5): the writer only
 * drafts; the browser saves drafts through the ordinary skill routes. */
export type ChatMessage = { role: 'person' | 'agent'; text: string };
/** A written skill not saved yet. `id` is the key saving uses; it changes with
 * the content, so a retry of the same text saves one skill. */
export type DraftSkill = { key: string; id: string; name: string; description: string; steps: string };
/** `id` comes with a draft made in chat: the skill's own, the same on every read. */
export type WrittenSkill = { name: string; description: string; steps: string; text: string; id?: string };
export type LibraryPick = { id: string; name: string; description: string };
/** `job_title` and `used` come only from a member's assistant: a title for an
 * agent that has none, and what it relied on from its memory or the Library. */
export type WriterAnswer = { reply: string; ground_rules: string | null; skills: WrittenSkill[]; remove: string[];
  reuse: LibraryPick[]; missing_apps: string[]; notes: string[]; job_title?: string | null; used?: string[] };
export type WriterRequest = { name: string; role: string; agent?: string; connections?: string[]; ground_rules: string;
  skills: { name: string; description: string; steps: string; saved?: boolean }[]; messages: ChatMessage[]; focus?: string };

const path = `/workspaces/${workspaceId}/skill-writer`;
export type ControlSuggestion = { connection: string; operation: string; mode: 'allow' | 'ask' | 'block'; reason: string };
export type ControlsRequest = { name: string; role: string; audience: string; agent?: string; connections?: string[]; ground_rules: string;
  skills: { name: string; description: string; steps: string }[] };
/** Suggested controls: never looser than safe defaults or the admin's rules. */
export const suggestControls = (request: ControlsRequest, signal?: AbortSignal) =>
  identityRequest<{ suggestions: ControlSuggestion[]; by: 'writer' | 'rules' }>(`${path}/controls`, request, signal);
export const loadWriter = () => identityRequest<{ available: boolean; guide: string }>(path);
export const askWriter = (request: WriterRequest) => identityRequest<WriterAnswer>(path, request);

/** The member's own assistant as the helper (schema 139). It is asked in its own
 * room; the drafts are read once it has answered, and nothing is saved by either. */
export type AssistantHelp = { available: boolean; assistant?: { id: string; name: string; avatar?: string | null }; guide?: string };
const help = `/workspaces/${workspaceId}/assistant/agent-setup`;
export const loadAssistantHelp = () => identityRequest<AssistantHelp>(help);
/** `id` is the turn's own: the same id asks once. */
export const askAssistant = (id: string, setup: WriterRequest) => identityRequest<{ reply: string }>(help, { id, setup });
/** What a draft made in your chat with your assistant adds: the new agent's
 * name and id, whether it exists already, and whether a later reply carried
 * the draft on. */
export type ChatDraft = { from?: 'chat'; name?: string | null; agent_id?: string; created?: boolean; replaced?: boolean };
export const readAssistantAnswer = (reply: string, signal?: AbortSignal) =>
  identityRequest<{ state: string } & Partial<WriterAnswer> & ChatDraft>(`${help}/${encodeURIComponent(reply)}`, undefined, signal);
export const saveSkillVersion = (id: string, version: number, files: SkillFile[]) =>
  identityRequest<SkillDetail>(`/workspaces/${workspaceId}/skills/${encodeURIComponent(id)}/versions`, { version, files });

/** A name in the Agent Skills format, as the server makes it. */
export function slug(name: string): string {
  let out = '';
  for (const char of name.trim().toLowerCase()) {
    if (/[a-z0-9]/.test(char)) out += char;
    else if (out && !out.endsWith('-')) out += '-';
  }
  return out.slice(0, 64).replace(/-+$/, '');
}

/** SKILL.md for a skill written here. */
export const skillText = (name: string, description: string, steps: string) =>
  `---\nname: ${name}\ndescription: ${description.replace(/\s+/g, ' ').trim()}\n---\n${steps.trim()}\n`;
/** The steps of a SKILL.md: everything after its header. */
export const stepsOf = (text: string) => text.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n?/, '').trim();

export const newDraft = (skill: Pick<DraftSkill, 'name' | 'description' | 'steps'>): DraftSkill =>
  ({ ...skill, key: crypto.randomUUID(), id: crypto.randomUUID() });

/** Drafts after a writer answer: changed ones keep their place, new ones join. */
export function applyAnswer(drafts: DraftSkill[], answer: Pick<WriterAnswer, 'skills' | 'remove'>): DraftSkill[] {
  let next = drafts.filter(draft => !answer.remove.includes(draft.name));
  for (const written of answer.skills) {
    const { name, description, steps } = written;
    next = next.some(draft => draft.name === name)
      ? next.map(draft => draft.name === name ? { ...draft, description, steps, id: crypto.randomUUID() } : draft)
      : [...next, newDraft({ name, description, steps })];
  }
  return next;
}

export type DiffLine = { kind: 'same' | 'removed' | 'added'; text: string };
/** Lines removed and added between two texts, by longest common subsequence. */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = before.split('\n'), b = after.split('\n');
  const common = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--)
    for (let j = b.length - 1; j >= 0; j--)
      common[i][j] = a[i] === b[j] ? common[i + 1][j + 1] + 1 : Math.max(common[i + 1][j], common[i][j + 1]);
  const lines: DiffLine[] = [];
  let i = 0, j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) { lines.push({ kind: 'same', text: a[i] }); i++; j++; }
    else if (i < a.length && (j === b.length || common[i + 1][j] >= common[i][j + 1])) lines.push({ kind: 'removed', text: a[i++] });
    else lines.push({ kind: 'added', text: b[j++] });
  }
  return lines;
}
