import { identityRequest, workspaceId } from '../../workspace-context';
import type { Mode } from '../agents/control-model';

export type Operation = { id: string; title: string; tool?: string; effect: 'read' | 'write'; schema?: unknown };
/** An app admins can approve: an operator-configured MCP server (`mcp`) or an app that signs in through its provider. */
export type Server = { id: string; name: string; account?: string | null; provider?: string; operations: Operation[] };
export type Connection = { id: string; server: string; definition: Server; enabled: boolean; revision: number; provider?: string; needs_reconnect?: boolean };
/** Connecting either saves at once or returns the provider's sign-in page to open. */
export type ConnectResult = { saved?: boolean; redirect_url?: string; provider_revoked?: boolean };
export type SystemApproval = { server: string; approved: boolean; revision: number };
export type Person = { id: string; name: string; role: string };
/** The admin's rule for each action of one connected account, and the agents using it. */
export type Safety = { connection: string; agents: string[]; operations: { id: string; title: string; effect: 'read' | 'write'; mode: Mode; needs_other: boolean; revision: number; default: boolean }[] };
export type WorkspaceConnectionsData = {
  servers: Server[]; systems: SystemApproval[]; connections: Connection[]; safety: Safety[];
  people: Person[]; viewer: string; execution_available: boolean; catalog_error?: string;
};
const root = `/workspaces/${workspaceId}/control`;
export const loadConnections = () => identityRequest<WorkspaceConnectionsData>(`${root}/connections`);
export const approveSystem = (input: SystemApproval) => identityRequest(`${root}/systems`, input);
export const connectAccount = (input: { id: string; server: string; enabled: boolean; revision: number }) => identityRequest<ConnectResult>(`${root}/connections`, input);
export const saveConnectorRule = (input: { connection: string; operation: string; mode: Mode; needs_other: boolean; revision: number }) => identityRequest(`${root}/connector-rules`, input);
