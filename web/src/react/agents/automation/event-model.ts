import { identityRequest, workspaceId } from '../../../workspace-context';
export type EventSource = { id: string; name: string; event_type: string; fields: string[]; binding: string };
type Authority = { connection_revision: number; read_revision: number; delegation_revision: number | null };
export type EventChoice = {
  source: EventSource;
  /** The conversation a trigger runs in (schema 151); none chosen: the person's direct chat. */
  chat: string | null;
  account?: string;
  account_name?: string;
  authority: Authority;
  /** Conversations where the person may ask the agent, so a trigger may run there. */
  conversations?: { id: string; name: string; visibility: string; kind?: string }[];
};
export type EventInput = Authority & {
  id: string;
  source: string;
  binding: string;
  chat?: string;
  name: string;
  instructions: string;
  filter: Record<string, unknown>;
};
export type EventTrigger = EventInput & {
  /** Its task (schema 148): a delivered event starts a run of it. */
  task?: string | null;
  state: string;
  revision: number;
  detail: string;
  principal_name: string;
  chat_name: string | null;
  /** It runs in the person's direct chat with the agent. */
  chat_direct?: boolean;
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
  conversations: { id: string; name: string; visibility: string; kind?: string }[];
  authority?: Authority;
};
/** An app event under Cuckoo's name, with the fields agents receive. */
export type AppEvent = { id: string; title: string; fields: string[] };
export type AppTriggerInput = Authority & {
  id: string;
  connection: string;
  event: string;
  chat?: string;
  name: string;
  instructions: string;
  filter: Record<string, unknown>;
};
export type Events = {
  triggers: EventTrigger[];
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
  chat: string | null;
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
/** Where the agent's owner may ask it, so an automation may run there, at most 100. */
export type PostTargets = { conversations: { id: string; name: string; kind: string }[]; more_conversations: boolean; execution_available: boolean };
export const loadPostTargets = (agent: string) =>
  identityRequest<PostTargets>(`/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(agent)}/post-targets`);
