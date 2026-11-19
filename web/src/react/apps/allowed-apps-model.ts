import { identityRequest, workspaceId } from '../../workspace-context';
import type { AddedFact } from './added-apps-model';

/** What the workspace's admins decided about an app (schema 134): members add
 * allowed apps, ask for undecided ones and can't use blocked ones. */
export type Allowance = 'allowed' | 'undecided' | 'blocked';
/** One app Cuckoo offers, in the admins' Allowed apps list. */
export type AllowedApp = { id: string; name: string; description: string; category: string; allowance: Allowance; note: string; revision: number;
  decided: { at: number; by_name: string } | null; connections: number; waiting: number;
  /** Set for an app the admins added from the library (schema 135). */
  added?: AddedFact | null };
/** A member's request for a catalog app, waiting for an admin. */
export type AppRequest = { id: string; kind: 'access' | 'suggestion'; connector: string | null; name: string; reason: string; created: number;
  requester: string; agent: string | null; agent_name: string | null };
/** The viewer's own request: waiting, or answered in the last seven days. */
export type MyAppRequest = { id: string; connector: string; name: string; state: 'pending' | 'approved' | 'declined'; note: string;
  created: number; decided: number | null; agent: string | null; agent_name: string | null };
/** An MCP server a member connected to their agent, waiting for an admin's yes. */
export type WaitingSystem = { id: string; name: string; url: string; tools: number; revision: number; created_ms: number;
  requester: string | null; agent: string; agent_name: string };
export type AppRequests = { requests: AppRequest[]; mine?: MyAppRequest[]; can_decide?: boolean; systems_waiting?: number };

const base = () => `/workspaces/${workspaceId}/apps`;
export const loadAllowedApps = () => identityRequest<{ apps: AllowedApp[]; systems?: WaitingSystem[]; library?: boolean }>(`${base()}/allowed`);
/** Allows an app for the workspace, or switches it off, which disconnects every agent's connection of it. */
export const saveAllowance = (input: { app: string; allowed: boolean; note: string; revision: number }) =>
  identityRequest<{ saved: true; provider_revoked: boolean }>(`${base()}/allowed`, input);
export const loadAppRequests = () => identityRequest<AppRequests>(`${base()}/requests`);
/** Asks the admins to allow a catalog app; `id` is the click's own, so a retry asks once. */
export const requestApp = (input: { id: string; connector: string; agent?: string; reason: string }) =>
  identityRequest<{ id: string; state: string }>(`${base()}/requests`, input);
export const decideAppRequest = (id: string, action: 'approve' | 'decline', note: string) =>
  identityRequest<{ state: string }>(`${base()}/requests/${encodeURIComponent(id)}`, { action, note });
