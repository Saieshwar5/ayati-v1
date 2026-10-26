export type Pin = { kind: 'chat' | 'local_agent'; id: string };
export type Preferences = { available: boolean; revision: number; pins: Pin[] };
export type Lifecycle = { archived: boolean; revision: number; manageable: boolean; active_requests: number };
export const samePin = (a: Pin, b: Pin) => a.kind === b.kind && a.id === b.id;
