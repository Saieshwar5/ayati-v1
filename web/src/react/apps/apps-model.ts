import { identityRequest, storageScope, workspaceId } from '../../workspace-context';
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
  connected?: { at: number; by_name: string } | null; via?: string };
/** A company system (AP2/AP3): opens through Cuckoo for the people allowed. */
export type CompanySystem = { id: string; kind: 'link'; name: string; description: string; audience: 'everyone' | 'selected'; revision: number;
  can_open: boolean; url?: string | null; owner?: string | null; owner_name?: string | null; can_decide: boolean;
  people?: { id: string; name: string }[] | null; requested: boolean };
/** An app built in Cuckoo (BA1): drafts show only to their builder and admins. */
export type BuiltApp = { id: string; kind: 'built'; name: string; description: string; audience: 'everyone' | 'selected'; version: number | null; draft_revision: number;
  builder_name: string; is_builder: boolean; can_open: boolean; can_manage: boolean; people?: { id: string; name: string }[] | null;
  versions?: { version: number; note: string; at: number }[] | null; helpers?: { id: string; name: string }[] | null };
export type AppCheck = { check: string; message: string };
export type AppFiles = { files: { path: string; size: number; content: string }[]; draft_revision: number; checks: AppCheck[];
  preview_errors?: { message: string; at: number }[] };
export type AppsView = { apps: WorkspaceApp[]; systems: CompanySystem[]; built?: BuiltApp[]; can_manage: boolean; requests_waiting: number };
export type AppRequestRow = { id: string; kind: 'access' | 'suggestion'; app?: string | null; app_name?: string | null; name: string; url: string; reason: string; created: number; requester: string };
/** One service in the Add app catalog, under Cuckoo's own names. */
export type CatalogApp = { id: string; name: string; description: string; category: string; kinds: Meaning[]; actions: number;
  state: 'connected' | 'available' | 'requested'; connection: string | null };
export type CatalogPage = { categories: { id: string; name: string; count: number }[]; apps: CatalogApp[]; next_cursor: string | null; can_manage: boolean };
const base = () => `/workspaces/${workspaceId}/apps`;
export const loadApps = () => identityRequest<AppsView>(base());
export const appsRequest = <T>(path: string, body?: unknown) => identityRequest<T>(`${base()}${path}`, body);
export function loadCatalog(search: { query: string; category: string; cursor?: string | null }, signal: AbortSignal) {
  const params = new URLSearchParams({ query: search.query, category: search.category });
  if (search.cursor) params.set('cursor', search.cursor);
  return identityRequest<CatalogPage>(`${base()}/catalog?${params}`, undefined, signal);
}
/** Approves the app, applies Cuckoo's safe rules and starts sign-in, in one request. */
export const connectApp = (app: string, request: string) => identityRequest<{ redirect_url?: string; connection?: string }>(`${base()}/connect`, { app, request });
/** Opening goes through Cuckoo, which checks access before sending the browser on. */
export const openUrl = (id: string) => `/api/identity${base()}/systems/${encodeURIComponent(id)}/open`;
export const builtPath = (id: string, path = '') => `/built/${encodeURIComponent(id)}${path}`;
export const frameUrl = (id: string, preview: boolean) => `/api/identity${base()}${builtPath(id, `/run${preview ? '?preview=true' : ''}`)}`;
export const downloadUrl = (id: string, version: number) => `/api/identity${base()}${builtPath(id, `/download?version=${version}`)}`;

/** Today's `status` stands in for `state` until the server sends it. */
export const appState = (app: { state?: AppState; status?: string }): AppState => app.state ?? (app.status === 'reconnect' ? 'reconnect' : 'ok');
/** Problems a person can fix: a sign-in to renew or an unconfirmed write to check. */
export const needsYou = (app: WorkspaceApp) => ['reconnect', 'writes_paused'].includes(appState(app));
/** Cuckoo's safe default for an action's kind: reading allowed, deleting blocked, the rest asks first. */
export const safeDefault = (meaning: Meaning): Mode => meaning === 'read' ? 'allow' : meaning === 'delete' ? 'block' : 'ask';
/** The kind Cuckoo suggests before an admin reviews it. Mirrors the server's
 * `relay_guess_meaning`; used only to preview rules before connecting. */
export function guessMeaning(operation: { id: string; title: string; effect: string }): Meaning {
  const text = `${operation.id} ${operation.title}`;
  if (operation.effect === 'read') return 'read';
  if (/(delete|remove|destroy|erase|purge|archive)/i.test(text)) return 'delete';
  if (/(refund|payment|pay[_ .-]|^pay|charge|invoice|transfer|payout|credit|debit)/i.test(text)) return 'money';
  if (/(send|e-?mail|message|reply|post|sms|whatsapp|notify|publish)/i.test(text)) return 'sends_outside';
  return 'change';
}
const gerunds: Record<Meaning, string> = { read: 'reading', change: 'changing', money: 'moving money', sends_outside: 'sending outside', delete: 'deleting' };
const verbs: Record<Mode, [string, string]> = { allow: ['is allowed', 'are allowed'], ask: ['asks first', 'ask first'], block: ['is blocked', 'are blocked'] };
/** "Reading is allowed; changing and sending outside ask first; deleting is blocked." */
export function rulesSummary(actions: { meaning: Meaning; mode: Mode }[]): string {
  const parts = (['allow', 'ask', 'block'] as Mode[]).flatMap(mode => {
    const kinds = [...new Set(actions.filter(a => a.mode === mode).map(a => gerunds[a.meaning]))];
    if (!kinds.length) return [];
    const list = kinds.length > 1 ? `${kinds.slice(0, -1).join(', ')} and ${kinds.at(-1)}` : kinds[0];
    return [`${list} ${verbs[mode][kinds.length > 1 ? 1 : 0]}`];
  });
  const text = parts.join('; ');
  return text ? `${text[0].toUpperCase()}${text.slice(1)}.` : '';
}

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

// The provider's sign-in page returns to Apps without naming the app, so the app
// being connected is remembered for this tab, per person and workspace.
const connectingKey = () => storageScope('apps.connecting');
export function rememberConnecting(app: string) {
  try { sessionStorage.setItem(connectingKey(), app); } catch { /* It only names the app on return. */ }
}
export function peekConnecting(): string | undefined {
  try { return sessionStorage.getItem(connectingKey()) ?? undefined; } catch { return undefined; }
}
export function forgetConnecting() {
  try { sessionStorage.removeItem(connectingKey()); } catch { /* Nothing was kept. */ }
}

const COLOURS = ['#4f46e5', '#0284c7', '#d97706', '#059669', '#7c3aed', '#dc4a26', '#2563eb', '#475569'];
/** A stable colour per app, so a card looks the same everywhere. */
export function appColour(key: string): string {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return COLOURS[hash % COLOURS.length];
}
