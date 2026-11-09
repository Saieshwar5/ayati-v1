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
  /** Conversations the work may be posted in; rules no longer pick one (schema 47). */
  conversations?: { id: string; name: string; visibility: string }[];
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
/** A connected app whose events could start this agent (schema 109). */
export type AppChoice = {
  connection: string;
  app: string;
  name: string;
  account?: string | null;
  /** Anything but `ready` says why its events can't start work, in `detail`. */
  state: 'ready' | 'reconnect' | 'unavailable' | 'no_read' | 'needs_allow';
  detail?: string | null;
  read_title?: string;
  chat?: string;
  chat_name?: string;
  visibility?: string;
  conversations: { id: string; name: string; visibility: string }[];
  authority?: Authority;
};
/** An app event under Cuckoo's name, with the fields agents receive. */
export type AppEvent = { id: string; title: string; fields: string[] };
export type AppTriggerInput = Authority & {
  id: string;
  connection: string;
  event: string;
  chat: string;
  name: string;
  instructions: string;
  filter: Record<string, unknown>;
};
/** A built app's trigger for this agent; its builder or an admin changes it in the app. */
export type AppTrigger = {
  app: string;
  app_name: string;
  collection: string;
  kind: 'added' | 'changed';
  instructions: string;
  by: string | null;
  manages: boolean;
};
export type Events = {
  triggers: EventTrigger[];
  app_triggers?: AppTrigger[];
  choices: EventChoice[];
  apps?: AppChoice[];
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
export const loadAppEvents = (connection: string) =>
  identityRequest<{ events: AppEvent[] }>(`/workspaces/${workspaceId}/apps/${encodeURIComponent(connection)}/events`);
