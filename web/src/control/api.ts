/** Cuckoo Control's request boundary: `/api/identity/admin/*` with the session
 * cookie. Writes carry the identity request header the server's guard needs. */

export class SignedOut extends Error {}
/** The signed-in person is not a Cuckoo operator. */
export class NotOperator extends Error {}

export async function control<T>(path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`/api/identity/admin${path}`, {
    method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin', signal,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Ayati-Request': 'identity' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json().catch(() => ({}));
  if (response.status === 401) throw new SignedOut(data.error || 'Sign in again.');
  if (response.status === 403) throw new NotOperator(data.error || 'Only Cuckoo operators can open Cuckoo Control.');
  if (!response.ok) throw new Error(data.error || 'Request could not be completed.');
  return data as T;
}

/** The existing invite-code API lives outside the admin prefix's helper. */
export async function codesRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/identity${path}`, {
    method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Ayati-Request': 'identity' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json().catch(() => ({}));
  if (response.status === 401) throw new SignedOut(data.error || 'Sign in again.');
  if (!response.ok) throw new Error(data.error || 'Request could not be completed.');
  return data as T;
}

export type Session = { email: string; name: string; environment: string; can_store_keys: boolean; catalog_on: boolean };

/** Charged: what people pay at catalog prices. Cost: what providers charge Cuckoo. */
export type Money = { charge: number; cost: number };
export type Alert = { level: 'bad' | 'warn' | 'info'; kind: string; title: string; body: string; target: string | null };
export type Overview = {
  workspaces: number; active_7d: number; people: number; calls_per_min: number; calls_1h: number; errors_1h: number;
  fell_back_1h: number; today: Money; month: Money; daily_ceiling: number | null;
  pool: { queued: number; oldest_queued_seconds: number; ready: number; busy: number; executors: number };
  traffic_start_ms: number; traffic: { provider: string; buckets: number[] }[];
  busiest: { id: string; name: string; running: number; queued: number; calls_5m: number; charge_today: number }[];
  alerts: Alert[];
  /** This server's live messaging over the last ten minutes. */
  messaging?: Messaging;
};
export type Timing = { count: number; p50_ms: number | null; p95_ms: number | null; max_ms: number | null };
export type Messaging = { open_streams: number; streams_opened: number; listening: boolean; listener_connects: number; delivery: Timing; sending: Timing; window_seconds: number };

export type WorkspaceStatus = 'active' | 'paused' | 'closing' | 'suspended' | 'closed';
export type WorkspaceRow = {
  id: string; name: string; created_ms: number | null; owner: string | null; people: number; agents: number; running: number;
  charge: number; cost: number; budget: number | null; calls_7d: number[]; error_rate_7d: number; last_active_ms: number | null;
  status: WorkspaceStatus;
};
export type LogEntry = { id?: number; summary: string; reason: string; at_ms: number; via: 'web' | 'cli'; by: string; area?: string; target?: string };
export type WorkspaceDetail = WorkspaceRow & {
  admins: number; connections: number; library_items: number; library_bytes: number; problem_reports_30d: number;
  code: string | null; suspended_ms: number | null; suspended_note: string | null; agents_paused_ms: number | null;
  models: string[]; log: LogEntry[];
};

export type Provider = {
  name: string; base_url?: string; source: 'console' | 'deployment' | 'missing'; enabled: boolean;
  key_in_console?: boolean; key_hint?: string; key_rotated_ms?: number;
  switch: { note: string; changed_ms: number; by: string } | null;
  health: { calls: number; errors: number; p95_ms: number | null };
  /** Catalog models this provider serves, from the reviewed catalog file. */
  routes: { model: string; provider_model: string; priority: number }[];
};
export type Providers = { providers: Provider[]; can_store_keys: boolean };

/** The operator model view (`GET /admin/models`): traffic, failures, margin and the catalog's routes. */
export type Usage = { key: string; calls: number; after_fallback: number; tokens: number; charge_usd: number; cost_usd: number; margin_usd: number; latency_avg_ms: number; latency_p95_ms: number };
export type Price = { input: number; output: number; cached_input?: number | null };
export type CatalogRoute = { provider: string; model: string; priority: number; weight: number; cost: Price; margin: Price };
export type Operations = {
  hours: number; providers: Usage[]; models: Usage[];
  failures: { provider: string; status: number; count: number; last_ms: number }[];
  resting: { provider: string; seconds: number }[];
  catalog: { on: boolean; error?: string; models: { id: string; name: string; status: string; price: Price; routes: CatalogRoute[] }[] };
};

export type Spend = {
  month_start_ms: number; month: Money & { margin: number }; today: Money; forecast: Money; daily_ceiling: number | null;
  daily: { day_ms: number; charge: number; cost: number; calls: number }[];
  by_model: { model: string; calls: number; input_tokens: number; output_tokens: number; charge: number; cost: number; margin: number }[];
  by_workspace: { id: string; name: string; charge: number; cost: number; margin: number; calls: number; budget: number | null; top_agent: { name: string; charge: number } | null }[];
};

export type ConsoleSetting = {
  key: string; about: string; kind: 'bool' | 'usd'; value: boolean | number | null; default: boolean | number | null;
  changed_ms: number | null; by: string | null; history: LogEntry[];
};
export type DeploymentSetting = { key: string; env: string; value: string | number | null; about: string };
export type Settings = { settings: ConsoleSetting[]; deployment: DeploymentSetting[] };

export type Report = { id: string; workspace: string; reporter: string; topic: string; body: string; created_ms: number; technical: unknown };

export type Code = {
  id: string; note: string; created_at: number; expires_at: number; status: 'unused' | 'used' | 'expired' | 'revoked';
  used_by?: { name: string; email: string } | null; workspace?: { id: string; name: string } | null;
};

export const money = (value: number | null | undefined, digits = 2) =>
  value === null || value === undefined ? '—'
    : `${value < 0 ? '−' : ''}$${Math.abs(value).toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits })}`;
export const count = (value: number) => value.toLocaleString('en-US');
export const when = (ms: number | null | undefined) => ms ? new Date(ms).toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : '—';
export const ago = (ms: number | null | undefined) => {
  if (!ms) return 'never';
  const minutes = Math.round((Date.now() - ms) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  if (minutes < 48 * 60) return `${Math.round(minutes / 60)} h ago`;
  return `${Math.round(minutes / 1440)} days ago`;
};
