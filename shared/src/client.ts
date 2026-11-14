/**
 * Requests to a Cuckoo controller from a client that is not a browser page:
 * the bearer token instead of a cookie, the request marker, and no cookies at
 * all, since a request with a cookie must also carry the browser's Origin
 * (runtime/mobile-sign-in.md).
 */
import type {
  AttentionPage,
  AttentionView,
  Device,
  DeviceSession,
  Directory,
  History,
  Conversation,
  Message,
  Session,
  Submission,
  UnreadResponse,
} from './types';

export type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

export class ApiError extends Error {
  constructor(message: string, readonly status: number) {
    super(message);
    this.name = 'ApiError';
  }
  /** The account can no longer see this: signed out, removed or not found. */
  get denied(): boolean {
    return [401, 403, 404].includes(this.status);
  }
}

export type ClientOptions = {
  /** `https://cuckoo.example` or local `http://127.0.0.1:8765`, without a trailing slash. */
  origin: string;
  fetch: Fetch;
  /** The device-session token, read for each request so sign-out applies at once. */
  token: () => string | undefined;
  /** Called on any 401, after which the app should forget the token. */
  onUnauthorized?: () => void;
};

export function headers(token: string | undefined, json: boolean): Record<string, string> {
  const result: Record<string, string> = { 'X-Ayati-Request': 'identity' };
  if (token) result.Authorization = `Bearer ${token}`;
  if (json) result['Content-Type'] = 'application/json';
  return result;
}

export function createClient(options: ClientOptions) {
  const request = async <T>(path: string, body?: unknown, signal?: AbortSignal): Promise<T> => {
    const response = await options.fetch(`${options.origin}/api/identity${path}`, {
      method: body === undefined ? 'GET' : 'POST',
      headers: headers(options.token(), body !== undefined),
      body: body === undefined ? undefined : JSON.stringify(body),
      credentials: 'omit',
      signal,
    });
    const data = await response.json().catch(() => ({}));
    if (!response.ok) {
      if (response.status === 401) options.onUnauthorized?.();
      const message = typeof data?.error === 'string' ? data.error : 'Request could not be completed.';
      throw new ApiError(message, response.status);
    }
    return data as T;
  };

  const workspace = (id: string) => {
    const base = `/workspaces/${encodeURIComponent(id)}`;
    return {
      directory: (signal?: AbortSignal) => request<Directory>(`${base}/chat-directory`, undefined, signal),
      chats: (signal?: AbortSignal) => request<{ chats: Conversation[] }>(`${base}/chats`, undefined, signal),
      unread: (signal?: AbortSignal) => request<UnreadResponse>(`${base}/unread`, undefined, signal),
      messages: (chat: string, before?: number, signal?: AbortSignal) =>
        request<History>(
          `${base}/chats/${encodeURIComponent(chat)}/messages?compact=true${before ? `&before=${before}` : ''}`,
          undefined,
          signal,
        ),
      send: (chat: string, submission: Submission, signal?: AbortSignal) =>
        request<{ message?: Message }>(`${base}/chats/${encodeURIComponent(chat)}/messages`, submission, signal),
      /** Create a conversation. `id` is chosen by the caller so a retry can't create two. */
      createChat: (input: { id: string; kind: 'direct'; name: string; users: string[]; agents: string[] }) =>
        request<{ id: string }>(`${base}/chats`, { ...input, description: '' }),
      receipts: (chat: string, ids: string[], seen: boolean) =>
        request<unknown>(`${base}/chats/${encodeURIComponent(chat)}/receipts`, { ids, seen }),
      attention: (view: AttentionView, before?: string | null, signal?: AbortSignal) => {
        const params = new URLSearchParams({ view });
        if (before) params.set('before', before);
        return request<AttentionPage>(`${base}/attention?${params}`, undefined, signal);
      },
      /** The live message stream's address, resuming after `cursor`. */
      liveUrl: (cursor?: string) =>
        `${options.origin}/api/identity${base}/message-events?compact=true${cursor ? `&after=${encodeURIComponent(cursor)}` : ''}`,
    };
  };

  return {
    request,
    workspace,
    session: (signal?: AbortSignal) => request<Session>('/session', undefined, signal),
    devices: (signal?: AbortSignal) => request<{ devices: Device[] }>('/devices', undefined, signal),
    revokeDevice: (id: string) =>
      request<{ ok: true; provider_session_revoked: boolean }>(`/devices/${encodeURIComponent(id)}/revoke`, {}),
    logout: () => request<{ ok: true }>('/logout', {}),
    /** Redeem the one-time sign-in code; needs no token. */
    exchange: (input: { code: string; verifier: string; device_name: string; platform: 'ios' | 'android' }) =>
      request<DeviceSession>('/mobile-sessions', input),
  };
}

export type Client = ReturnType<typeof createClient>;
export type WorkspaceClient = ReturnType<Client['workspace']>;
