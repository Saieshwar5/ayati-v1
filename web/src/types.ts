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
}

export interface AgentEvent {
  kind: 'state' | 'delta' | 'resync';
  task_id?: string;
  message_id?: string;
  text?: string;
  offset?: number;
}

export async function api<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`/api${path}`, options);
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
