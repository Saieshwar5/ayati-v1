/**
 * The controller's JSON shapes the phone app reads. They mirror the web
 * client's types (web/src/workspace-context.ts, web/src/react/chat/model.ts,
 * web/src/react/attention/model.ts); fields the app does not use are left out.
 */

export type User = { id: string; email: string; display_name?: string | null };

export type Membership = {
  id: string;
  name: string;
  role: string;
  people?: number;
  agents?: number;
  colour?: string | null;
  timezone?: string | null;
  agents_paused_ms?: number | null;
  suspended?: boolean;
};

export type Invitation = { id: string; name: string; role: string; expires: number; inviter?: string | null };

/** `GET /api/identity/session`. `user` is null when signed out. */
export type Session = {
  user: User | null;
  workspaces?: Membership[];
  invitations?: Invitation[];
};

export type Participant = {
  id: string;
  name: string;
  email?: string;
  role?: string;
  title?: string;
  about?: string;
  photo?: string | null;
  avatar?: string | null;
  manager?: string | null;
};

export type Presence = { user: string; online?: boolean; status?: string | null };

/** `GET …/chat-directory`: who the viewer can talk to. */
export type Directory = { users: Participant[]; agents: Participant[]; role: string; presence?: Presence[] };

export type Conversation = {
  id: string;
  name: string;
  kind: 'group' | 'channel' | 'direct';
  description: string;
  users: string[];
  agents: string[];
  archived?: boolean;
  /** Seconds of the newest activity, as the server ordered the list. */
  activity?: number;
};

export type Progress = {
  state: 'starting' | 'waiting' | 'responding' | 'interrupted' | 'approval' | 'input' | 'resuming' | 'attention' | 'handoff';
  text: string;
  generation?: number;
};

export type Attachment = {
  id: string;
  name: string;
  mime: string;
  size: number;
  transcript?: string | null;
  /** Audio only (schema 95); `not_enabled` where the deployment doesn't transcribe. */
  transcript_status?: 'pending' | 'ready' | 'failed' | 'not_enabled';
  duration_ms?: number | null;
};

export type Message = {
  id: string;
  author: string;
  name: string | null;
  kind: string;
  content: string;
  state: string;
  /** Seconds. */
  at: number;
  sequence?: number;
  revision?: number;
  can_cancel?: boolean;
  chat?: string;
  thread?: string | null;
  reply_to?: string | null;
  unread?: boolean;
  for_me?: boolean;
  progress?: Progress | null;
  attachments?: Attachment[];
  edited_at?: number | null;
  /** The exact action this reply asks approval for. */
  approval?: import('./decisions').ApprovalReference;
  /** Questions this reply asks. */
  questions?: import('./decisions').QuestionReference[];
};

/** `GET …/chats/{chat}/messages`. */
export type History = { messages: Message[]; agents: string[]; next_before?: number | null };

/** What `POST …/chats/{chat}/messages` takes. The same `id` keeps a retry from posting twice. */
export type Submission = {
  id: string;
  content: string;
  agents?: string[];
  thread?: string;
  reply_to?: string;
  attachments: string[];
};

export type UnreadResponse = { unread: { chat: string; count: number; mentions?: number }[] };

export type AttentionView = 'needs_you' | 'recent';
export type AttentionItem = {
  kind: 'question' | 'approval' | 'outcome';
  id: string;
  version: string;
  title: string;
  status: string;
  at: number;
  agent: string;
  agent_name: string;
  chat: string;
  thread: string | null;
  reply: string;
  conversation_name: string;
  archived: boolean;
  read: boolean;
  reviewed: boolean;
  operation_title?: string;
};
export type AttentionPage = { available: boolean; items: AttentionItem[]; next_before: string | null };

/** A signed-in phone (`GET /api/identity/devices`). Times are seconds. */
export type Device = {
  id: string;
  name: string;
  platform: 'ios' | 'android';
  created: number;
  last_used: number;
  expires: number;
  current: boolean;
};

/** `POST /api/identity/mobile-sessions`. */
export type DeviceSession = { token: string; id: string; expires: number };

/** One batch from the live message stream. */
export type Change = {
  kind: 'message' | 'access' | 'reset' | 'sample_access' | 'event_access' | 'panel_access' | 'agents';
  source?: string;
  chat?: string;
  message?: Message;
};
export type ChangeBatch = { cursor: number; reset?: boolean; more?: boolean; events: Change[] };
