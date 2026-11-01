import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import type { PanelDefinition } from '../../panel-types';
export type Snapshot = {
  revision: number;
  available: boolean;
  definition?: PanelDefinition;
  saved_ms?: number;
  source_operations?: number;
  chat?: string;
  thread?: string | null;
  task?: string;
  checked_ms?: number;
  generation?: number;
  /** The agent's note on what changed with this update, when it wrote one. */
  what_changed?: string | null;
  /** Published by the controller from the source, without the agent. */
  from_source?: boolean;
};
export type Maintenance = {
  revision: number; panel_revision: number; enabled: boolean; eligible: boolean;
  execution_available: boolean; instructions: string; interval_seconds: number | null;
  stale_seconds: number; scopes: {connection: string; operation: string; arguments: unknown}[] | null;
  status: string; checked_ms: number | null; saved_ms: number | null; next_ms: number | null;
  /** The latest update request, opened from a delayed or failed panel. */
  last_update?: string | null;
  /** Also update after the agent's work or an event on its sources, at most every min_gap_seconds. */
  follow_sources?: boolean; min_gap_seconds?: number;
  /** Numbers refresh from the source without the agent (saved bindings). */
  from_source?: boolean;
  last_day?: { updates: number; without_agent: number };
};
export type PersonalPanel = {
  id: string;
  revision: number;
  active_revision: number | null;
  agent_name: string | null;
  agent?: string | null;
  active: Snapshot | null;
  draft: Snapshot | null;
  maintenance?: Maintenance | null;
};
export type HomeLayout = { revision: number; order: string[]; wide: string[]; hidden: string[] };
export type PanelsPage = { available: boolean; panels: PersonalPanel[]; layout: HomeLayout };
export const base = `/workspaces/${workspaceId}/home-panels`;
export const readPanels = (signal?: AbortSignal) => identityRequest<PanelsPage>(base, undefined, signal);
export const saveLayout = (layout: HomeLayout, request: string) =>
  identityRequest<HomeLayout>(`${base}/layout`, { ...layout, request });
export const keepPanel = (id: string, revision: number, request: string) =>
  identityRequest(`${base}/${encodeURIComponent(id)}/review`, { request, revision });
export const saveAgreement = (id: string, input: {
  request: string; revision: number; panel_revision: number; enabled: boolean;
  instructions: string; interval_seconds: number | null; stale_seconds: number;
  follow_sources?: boolean; min_gap_seconds?: number;
}) => identityRequest(`${base}/${encodeURIComponent(id)}/maintenance`, input);
export const requestUpdate = (id: string, revision: number, request: string) =>
  identityRequest(`${base}/${encodeURIComponent(id)}/refresh`, {request, revision});
export const sourceUrl = (snapshot: Snapshot) =>
  workspaceUrl(
    `/?view=chat&conversation=${encodeURIComponent(snapshot.chat!)}&message=${encodeURIComponent(snapshot.task!)}${snapshot.thread ? `&thread=${encodeURIComponent(snapshot.thread)}` : ''}`,
  );

export type ChangeTarget = { id: string; revision: number; agent: string; chat: string };
