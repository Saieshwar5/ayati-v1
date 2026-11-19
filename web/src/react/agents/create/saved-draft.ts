import { identityRequest, workspaceId } from '../../../workspace-context';
import type { Agent, Audience } from '../model';
import { readAssistantAnswer, type ChatMessage, type LibraryPick } from '../skills/writer-model';
import { defaultRobot } from '../../faces';

/** New agent as far as the person got (schema 139). It is kept on the server,
 * one per person and workspace, so a reload or another device does not lose
 * it. Keeping it creates nothing. An uploaded photo is not kept; a chosen
 * picture is. */
export type SavedDraft = { id: string; name: string; role: string; avatar: string; instructions: string;
  audience: Exclude<Audience, 'only_me'>; people: string[]; step: number;
  skills: { id: string; name: string; description: string; steps: string }[]; library: LibraryPick[]; messages: ChatMessage[] };

const path = `/workspaces/${workspaceId}/agent-draft`;
const list = <T,>(value: unknown): T[] => Array.isArray(value) ? value as T[] : [];

/** The person's unfinished agent, or nothing. An answer that is not a draft is nothing. */
export async function loadSavedDraft(): Promise<SavedDraft | undefined> {
  const { draft } = await identityRequest<{ draft?: Partial<SavedDraft> | null }>(path);
  if (!draft || typeof draft.id !== 'string' || typeof draft.name !== 'string') return undefined;
  return { id: draft.id, name: draft.name, role: draft.role ?? '', avatar: draft.avatar ?? '', instructions: draft.instructions ?? '',
    audience: draft.audience === 'selected' ? 'selected' : 'everyone', people: list(draft.people),
    step: [1, 2].includes(Number(draft.step)) ? Number(draft.step) : 0,
    skills: list(draft.skills), library: list(draft.library), messages: list(draft.messages) };
}
export const keepDraft = async (draft: SavedDraft) => { await identityRequest(path, draft); };
/** Drops the draft with this id; a newer one is kept. */
export const discardDraft = async (id: string) => { await identityRequest(`${path}/discard`, { id }); };

/** A draft your assistant made in your chat with it, to review in New agent.
 * Nothing when it is not finished, not from chat, or its agent already exists. */
export async function loadChatDraft(reply: string): Promise<SavedDraft | undefined> {
  const draft = await readAssistantAnswer(reply);
  if (draft.state !== 'complete' || draft.from !== 'chat' || !draft.agent_id || draft.created) return undefined;
  return { id: draft.agent_id, name: draft.name ?? '', role: draft.job_title ?? '', avatar: defaultRobot(draft.agent_id),
    instructions: draft.ground_rules ?? '', audience: 'everyone', people: [], step: draft.name ? 1 : 0,
    skills: (draft.skills ?? []).map(({ id, name, description, steps }) => ({ id: id ?? crypto.randomUUID(), name, description, steps })),
    library: draft.reuse ?? [], messages: [] };
}

/** The agent a kept draft carries on from, on top of a fresh one's defaults. */
export function agentFrom(saved: SavedDraft, fresh: Agent): Agent {
  return { ...fresh, id: saved.id, name: saved.name, role: saved.role, audience: saved.audience,
    settings: { ...fresh.settings, avatar: saved.avatar || fresh.settings.avatar, instructions: saved.instructions } };
}
