import { chatRequest } from '../chat/model';

// Memory (schema 142): notes agents and people keep. The workspace's memory is
// read by admins in Settings; an agent's job and room notes by its owners and
// admins on its page; what an agent remembers about you, by you alone.
export type MemoryKind = 'customer' | 'system' | 'people' | 'decision' | 'practice' | 'preference' | 'status' | 'term';
export const kinds: [MemoryKind, string][] = [
  ['customer', 'Customer'], ['system', 'System'], ['people', 'People and roles'], ['decision', 'Decision'],
  ['practice', 'Way of working'], ['preference', 'Preference'], ['status', 'Status'], ['term', 'What a word means'],
];
export const kindName = (kind: string) => kinds.find(([id]) => id === kind)?.[1] ?? kind;

export type Note = {
  id: string; scope: string; kind: MemoryKind; topic: string; text: string; source: string;
  by?: string | null; room?: string | null; learned_in?: { id: string; name: string; kind: string } | null;
  created_ms: number; changed_ms?: number | null; used: number; used_ms: number; strength: number;
  pinned: boolean; essential: boolean; until_ms?: number | null; state: string; waiting_reason?: string | null;
  replaces?: { id: string; text: string; used: number; created_ms: number; pinned: boolean } | null;
};
export type Topic = { topic: string; notes: number; used: number; updated_ms: number; strength: number; pinned: boolean; summary?: string | null; lead?: string | null };
export type Change = { id: string; action: string; summary: string; at_ms: number; undone_ms?: number | null };
export type Missed = { query: string; times: number; agents: string[]; last_ms: number };
export type View = 'topics' | 'notes' | 'essentials' | 'fading' | 'archive' | 'waiting' | 'changes' | 'missed';
export type Counts = { notes: number; topics: number; essentials: number; fading: number; archive: number; waiting: number };
export type MemoryPage = {
  available: boolean; view?: View; items?: unknown[]; counts?: Counts; used_month?: number;
  /** Topic summaries are written by a model on this Cuckoo. */
  summaries?: boolean;
};
export type NoteDraft = { id: string; kind: MemoryKind; topic: string; text: string; until?: string | null };
/** pin, unpin, confirm, essential, unessential, end, restore, remove, keep or discard. */
export type NoteChange = 'pin' | 'unpin' | 'confirm' | 'essential' | 'unessential' | 'end' | 'restore' | 'remove' | 'keep' | 'discard';

const base = (agent?: string) => agent ? `/agent-profiles/${encodeURIComponent(agent)}/memory` : '/memory';
export function loadMemory(agent: string | undefined, view: View, search = '', topic = '') {
  const query = new URLSearchParams({ view });
  if (search) query.set('q', search);
  if (topic) query.set('topic', topic);
  return chatRequest<MemoryPage>(`${base(agent)}?${query}`);
}
export const saveNote = (agent: string | undefined, note: NoteDraft) => chatRequest<{ ok: true }>(base(agent), note);
export const changeNote = (agent: string | undefined, id: string, change: NoteChange) =>
  chatRequest<{ ok: true }>(`${base(agent)}/notes/${encodeURIComponent(id)}/${change}`, {});
export const saveTopic = (agent: string | undefined, topic: string, summary: string) => chatRequest<{ ok: true }>(`${base(agent)}/topics`, { topic, summary });
export const undoChange = (agent: string | undefined, id: string) => chatRequest<{ ok: true }>(`${base(agent)}/changes/${encodeURIComponent(id)}/undo`, {});
export const loadAboutMe = (agent: string) => chatRequest<{ available: boolean; notes: Note[] }>(`/agent-profiles/${encodeURIComponent(agent)}/memory/about-me`);
export const forgetAboutMe = (agent: string, id: string) => chatRequest<{ ok: true }>(`/agent-profiles/${encodeURIComponent(agent)}/memory/about-me/${encodeURIComponent(id)}/remove`, {});

/** A note's strength as a word: never colour alone. */
export function strengthWord(strength: number): string {
  return strength >= 70 ? 'Strong' : strength >= 30 ? 'Steady' : strength >= 10 ? 'Fading' : 'Weak';
}
export const reasons: Record<string, string> = {
  private_source: 'Learned in a conversation not everyone can open',
  outside: 'Learned from people outside the workspace',
  background: 'Learned while working on its own',
  disagreement: 'Corrects a note',
};
export const day = (ms: number) => new Date(ms).toLocaleDateString([], { day: 'numeric', month: 'short', year: new Date(ms).getFullYear() === new Date().getFullYear() ? undefined : 'numeric' });
