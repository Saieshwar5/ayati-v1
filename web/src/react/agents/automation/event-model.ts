import { identityRequest, workspaceId } from '../../../workspace-context';
export type EventSource = { id: string; name: string; event_type: string; fields: string[]; binding: string };
type Authority = { connection_revision: number; read_revision: number; delegation_revision: number | null };
export type EventChoice = {
  source: EventSource;
  chat: string;
  chat_name: string;
  visibility: string;
  account?: string;
  account_name?: string;
  authority: Authority;
};
export type EventInput = Authority & {
  id: string;
  source: string;
  binding: string;
  chat: string;
  name: string;
  instructions: string;
  filter: Record<string, unknown>;
};
export type EventTrigger = EventInput & {
  state: string;
  revision: number;
  detail: string;
  principal_name: string;
  chat_name: string;
  instructions_long: boolean;
  can_resume: boolean;
  choice?: EventChoice | null;
  source_definition: EventSource;
};
export type Events = {
  triggers: EventTrigger[];
  choices: EventChoice[];
  execution_available: boolean;
  catalog_error?: string;
};
export type Delivery = {
  id: string;
  state: string;
  revoked: boolean;
  detail: string;
  received_ms: number;
  chat: string;
  reply?: string;
  task_state?: string;
  job_state?: string;
  progress?: { state?: string };
};
export const eventPath = (agent: string, id?: string) =>
  `/workspaces/${workspaceId}/agent-profiles/${agent}/event-triggers${id ? `/${id}` : ''}`;
export const loadEvents = (agent: string) => identityRequest<Events>(eventPath(agent));
