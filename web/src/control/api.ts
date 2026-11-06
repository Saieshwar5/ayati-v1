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

export type Session = { email: string; name: string; environment: string; can_store_keys: boolean };

export type Alert = { level: 'bad' | 'warn' | 'info'; kind: string; title: string; body: string; target: string | null };
export type Overview = {
  workspaces: number; active_7d: number; people: number; calls_per_min: number; calls_1h: number; errors_1h: number;
  fell_back_1h: number; spend_today: number; spend_month: number; daily_ceiling: number | null;
  pool: { queued: number; oldest_queued_seconds: number; ready: number; busy: number; executors: number };
  traffic_start_ms: number; traffic: { provider: string; buckets: number[] }[];
  busiest: { id: string; name: string; running: number; queued: number; calls_5m: number; spend_today: number }[];
  alerts: Alert[];
};

export type WorkspaceStatus = 'active' | 'paused' | 'closing' | 'suspended' | 'closed';
export type WorkspaceRow = {
  id: string; name: string; created_ms: number | null; owner: string | null; people: number; agents: number; running: number;
  spend: number; cap: number | null; own_cap: boolean; calls_7d: number[]; error_rate_7d: number; last_active_ms: number | null;
  status: WorkspaceStatus;
};
export type LogEntry = { id?: number; summary: string; reason: string; at_ms: number; via: 'web' | 'cli'; by: string; area?: string; target?: string };
export type WorkspaceDetail = WorkspaceRow & {
  admins: number; connections: number; library_items: number; library_bytes: number; problem_reports_30d: number;
  code: string | null; suspended_ms: number | null; suspended_note: string | null; agents_paused_ms: number | null;
  models: string[]; log: LogEntry[];
};

export type CatalogModel = { provider: string; model: string; enabled: boolean; input_per_mtok: number; output_per_mtok: number; changed_ms: number };
export type Provider = {
  name: string; base_url?: string; source: 'console' | 'deployment' | 'missing'; enabled: boolean;
  key_hint?: string; key_rotated_ms?: number;
  switch: { note: string; changed_ms: number; by: string } | null;
  health: { calls: number; errors: number; p95_ms: number | null }; models: CatalogModel[];
};
export type Providers = { providers: Provider[]; can_store_keys: boolean };

export type RouteRow = {
  model: string; main_provider: string; main_model: string; second_provider: string | null; second_model: string | null;
  main_percent: number; timeout_seconds: number; changed_ms: number; by: string;
  traffic: { provider: string; model: string; calls: number; errors: number; p95_ms: number | null }[];
};
export type Routes = { routes: RouteRow[]; requested: { model: string; agents: number; calls_7d: number }[] };

export type Spend = {
  month_start_ms: number; month: number; today: number; forecast: number; daily_ceiling: number | null; default_cap: number | null;
  daily: { day_ms: number; cost: number; calls: number }[];
  by_model: { provider: string; model: string; calls: number; input_tokens: number; output_tokens: number; cost: number; unpriced_calls: number }[];
  by_workspace: { id: string; name: string; cost: number; calls: number; cap: number | null; top_agent: { name: string; cost: number } | null }[];
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
  value === null || value === undefined ? '—' : `$${value.toLocaleString('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits })}`;
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
