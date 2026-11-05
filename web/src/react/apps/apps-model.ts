import { identityRequest, workspaceId } from '../../workspace-context';
import type { Mode } from '../agents/control-model';
import type { Meaning } from '../settings/workspace-connections-model';
import type { AgentIdentity } from '../faces';

/** One connected service on the Apps page (AP1): what agents may do with each action. */
export type WorkspaceApp = { id: string; server: string; name: string; kind: 'service'; provider: string; account?: string | null; status: 'connected' | 'reconnect';
  actions: { id: string; title: string; meaning: Meaning; mode: Mode }[]; agents: AgentIdentity[]; rules: number };
export type AppsView = { apps: WorkspaceApp[]; can_manage: boolean };
export const loadApps = () => identityRequest<AppsView>(`/workspaces/${workspaceId}/apps`);

const COLOURS = ['#4f46e5', '#0284c7', '#d97706', '#059669', '#7c3aed', '#dc4a26', '#2563eb', '#475569'];
/** A stable colour per app, so a card looks the same everywhere. */
export function appColour(key: string): string {
  let hash = 0;
  for (const char of key) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return COLOURS[hash % COLOURS.length];
}
