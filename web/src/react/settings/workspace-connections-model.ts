import { identityRequest, workspaceId } from '../../workspace-context';
import type { Connection, Delegation, Server } from '../agents/control-model';

export type SystemApproval = { server: string; approved: boolean; revision: number };
export type Creator = { id: string; name: string; role: string; can_create: boolean; revision: number };
export type WorkspaceConnectionsData = {
  servers: Server[]; systems: SystemApproval[]; connections: Connection[];
  people: Creator[]; delegations: Delegation[]; viewer: string; execution_available: boolean;
  catalog_error?: string;
};
const root = `/workspaces/${workspaceId}/control`;
export const loadConnections = () => identityRequest<WorkspaceConnectionsData>(`${root}/connections`);
export const approveSystem = (input: SystemApproval) => identityRequest(`${root}/systems`, input);
export const connectAccount = (input: { id: string; server: string; enabled: boolean; revision: number }) => identityRequest(`${root}/connections`, input);
export const saveCreator = (person: Creator, enabled: boolean) => identityRequest(`${root}/creators`, { user: person.id, enabled, revision: person.revision });
export const saveDelegation = (input: { user: string; connection: string; operation: string; ceiling: string; revision: number; connection_revision: number }) => identityRequest(`${root}/delegations`, input);

export const ceilings = { allow: 'May allow automatically', ask: 'Must ask me first', block: 'Unavailable' };
