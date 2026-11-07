export type Pin = { kind: 'chat'; id: string };
export type Preferences = { available: boolean; revision: number; pins: Pin[] };
/** An unfinished request in a channel, as the archive review lists it. */
export type Unfinished = {
  request: string; agent: string; agent_name: string | null; agent_avatar: string | null; summary: string | null;
  state: 'queued' | 'running' | 'waiting' | 'approval' | 'question'; started_ms: number | null;
};
/** `unfinished` (the first 20) and `more` come only to people who may archive. */
export type Lifecycle = { archived: boolean; revision: number; manageable: boolean; active_requests: number; unfinished?: Unfinished[]; more?: number };
export const samePin = (a: Pin, b: Pin) => a.kind === b.kind && a.id === b.id;
