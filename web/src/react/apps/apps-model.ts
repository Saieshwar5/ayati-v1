import { identityRequest, workspaceId } from '../../workspace-context';
import type { Mode } from '../agents/control-model';
import type { Meaning } from '../settings/workspace-connections-model';
import type { AgentIdentity } from '../faces';

/** One connected service on the Apps page (AP1): what agents may do with each action. */
export type WorkspaceApp = { id: string; server: string; name: string; kind: 'service'; provider: string; account?: string | null; status: 'connected' | 'reconnect';
  actions: { id: string; title: string; meaning: Meaning; mode: Mode }[]; agents: AgentIdentity[]; rules: number };
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
const base = () => `/workspaces/${workspaceId}/apps`;
export const loadApps = () => identityRequest<AppsView>(base());
export const appsRequest = <T>(path: string, body?: unknown) => identityRequest<T>(`${base()}${path}`, body);
/** Opening goes through Cuckoo, which checks access before sending the browser on. */
export const openUrl = (id: string) => `/api/identity${base()}/systems/${encodeURIComponent(id)}/open`;
export const builtPath = (id: string, path = '') => `/built/${encodeURIComponent(id)}${path}`;
export const frameUrl = (id: string, preview: boolean) => `/api/identity${base()}${builtPath(id, `/run${preview ? '?preview=true' : ''}`)}`;
export const downloadUrl = (id: string, version: number) => `/api/identity${base()}${builtPath(id, `/download?version=${version}`)}`;

const COLOURS = ['#4f46e5', '#0284c7', '#d97706', '#059669', '#7c3aed', '#dc4a26', '#2563eb', '#475569'];
/** A stable colour per app, so a card looks the same everywhere. */
export function appColour(key: string): string {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return COLOURS[hash % COLOURS.length];
}
