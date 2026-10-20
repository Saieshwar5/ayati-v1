import { workspaceUrl, workspaceId } from './workspace-context.ts';
export interface ChatMessage {
  id: string;
  task_id: string;
  role: 'user' | 'assistant';
  content: string;
  state: string;
  created_at: string;
}

export interface Task {
  id: string;
  status: string;
  detail: string;
  created_at: string;
}

export interface Artifact {
  id: string;
  task_id: string;
  name: string;
  kind: 'upload' | 'output' | 'log';
  size: number;
  created_at: string;
}

export interface Snapshot {
  messages: ChatMessage[];
  tasks: Task[];
  artifacts: Artifact[];
  permissions?: { id: string; task_id: string; title: string; action: unknown }[];
}

export interface AgentEvent {
  kind: 'state' | 'delta' | 'resync';
  task_id?: string;
  message_id?: string;
  text?: string;
  offset?: number;
}

export const selectedAgent = new URLSearchParams(location.search).get('agent') ?? 'local-agent';
export function apiUrl(path: string): string {
  return `${workspaceUrl(`/api${path}`)}${path.includes('?') ? '&' : '?'}agent=${encodeURIComponent(selectedAgent)}`;
}

export async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const headers = new Headers(options?.headers);
  if (!headers.has('X-Ayati-Agent')) headers.set('X-Ayati-Agent', selectedAgent);
  if (workspaceId && options?.method && options.method !== 'GET') headers.set('X-Ayati-Request', 'identity');
  const response = await fetch(workspaceUrl(`/api${path}`), { ...options, headers });
  if (response.status === 401) { location.assign('/?view=account&choose=1'); throw new Error('Please sign in again.'); }
  const data = await response.json();
  if (!response.ok) throw new Error(data.error ?? 'Something went wrong. Please try again.');
  return data as T;
}

export function post<T>(path: string, body?: unknown): Promise<T> {
  return api<T>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body ?? {}),
  });
}
