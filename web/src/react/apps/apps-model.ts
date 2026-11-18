import { identityRequest, workspaceId } from '../../workspace-context';
import type { Mode } from '../agents/control-model';
import type { Meaning } from '../settings/workspace-connections-model';
import type { AgentIdentity } from '../faces';

/** How a connected service is doing (designs/connector-layer/contracts.md, schema 108):
 * `paused` means Cuckoo switched its connection service off for a while. */
export type AppState = 'ok' | 'reconnect' | 'writes_paused' | 'paused';
/** One connected service on the Apps page (AP1): what agents may do with each action.
 * The fields after `rules` arrive with schema 108; older servers leave them out. */
export type WorkspaceApp = { id: string; server: string; name: string; kind: 'service'; provider: string; account?: string | null; status: 'connected' | 'reconnect';
  actions: { id: string; title: string; meaning: Meaning; mode: Mode }[]; agents: AgentIdentity[]; rules: number;
  state?: AppState; paused_action?: string | null; last_used?: { at: number; agent_name: string } | null;
  connected?: { at: number; by_name: string } | null; via?: string;
  /** The agent it belongs to (schema 132); none for a workspace connection. */
  owner?: { id: string; name: string } | null; revision?: number };
/** One service in the Add app catalog, under Cuckoo's own names. */
export type CatalogApp = { id: string; name: string; description: string; category: string; kinds: Meaning[]; actions: number;
  state: 'connected' | 'available' | 'requested'; connection: string | null };
export type CatalogPage = { categories: { id: string; name: string; count: number }[]; apps: CatalogApp[]; next_cursor: string | null; can_manage: boolean };
const base = () => `/workspaces/${workspaceId}/apps`;
/** Every connected app and MCP server in the workspace, for the admins' list in Settings. */
export const loadApps = () => identityRequest<{ apps: WorkspaceApp[]; can_manage: boolean }>(base());
/** With `agent`, an app counts as connected when that agent has its own
 * connection of it (schema 132). */
export function loadCatalog(search: { query: string; category: string; cursor?: string | null; agent?: string }, signal: AbortSignal) {
  const params = new URLSearchParams({ query: search.query, category: search.category });
  if (search.cursor) params.set('cursor', search.cursor);
  if (search.agent) params.set('agent', search.agent);
  return identityRequest<CatalogPage>(`${base()}/catalog?${params}`, undefined, signal);
}
/** Approves the app, applies Cuckoo's safe rules and starts sign-in, in one request.
 * With `agent` the connection belongs to that agent alone and its owner may make it. */
export const connectApp = (app: string, request: string, agent?: string) =>
  identityRequest<{ redirect_url?: string; connection?: string }>(`${base()}/connect`, agent ? { app, request, agent } : { app, request });
/** Today's `status` stands in for `state` until the server sends it. */
export const appState = (app: { state?: AppState; status?: string }): AppState => app.state ?? (app.status === 'reconnect' ? 'reconnect' : 'ok');
/** Problems a person can fix: a sign-in to renew or an unconfirmed write to check. */
export const needsYou = (app: WorkspaceApp) => ['reconnect', 'writes_paused'].includes(appState(app));
/** Cuckoo's safe default for an action's kind: reading allowed, deleting blocked, the rest asks first. */
export const safeDefault = (meaning: Meaning): Mode => meaning === 'read' ? 'allow' : meaning === 'delete' ? 'block' : 'ask';
/** The contract leaves time units open: control actions use seconds, newer fields may use ms. */
export const toMs = (at: number) => at < 1e12 ? at * 1000 : at;
function startOfDay(ms: number) { const day = new Date(ms); day.setHours(0, 0, 0, 0); return day.getTime(); }
/** "today", "yesterday" or "2 Nov". */
export function dayName(at: number): string {
  const ms = toMs(at), today = startOfDay(Date.now()), day = startOfDay(ms);
  if (day === today) return 'today';
  if (day === today - 86400000) return 'yesterday';
  return new Date(ms).toLocaleDateString([], { day: 'numeric', month: 'short' });
}
/** "just now", "10 min ago", "3 hours ago", then the day. */
export function agoText(at: number): string {
  const minutes = Math.max(0, Math.round((Date.now() - toMs(at)) / 60000));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  return hours < 12 ? `${hours} hour${hours === 1 ? '' : 's'} ago` : dayName(at);
}
export const clockTime = (at: number) => new Date(toMs(at)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });

const COLOURS = ['#4f46e5', '#0284c7', '#d97706', '#059669', '#7c3aed', '#dc4a26', '#2563eb', '#475569'];
/** A stable colour per app, so a card looks the same everywhere. */
export function appColour(key: string): string {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return COLOURS[hash % COLOURS.length];
}
