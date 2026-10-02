import type { Artifact, ChatMessage, Snapshot, Task } from './types';

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K, className = '', text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function renderConversation(container: HTMLElement, state: Snapshot): void {
  const atEnd = container.scrollHeight - container.scrollTop - container.clientHeight < 100;
  const nodes: HTMLElement[] = [];
  if (!state.messages.length) {
    const welcome = element('section', 'welcome');
    welcome.append(element('div', 'welcome-mark', 'a'),
      element('h1', '', 'A little help, whenever you need it.'),
      element('p', '', 'Ask a question, work through an idea, or share a file.'));
    nodes.push(welcome);
  }
  const tasks = new Map(state.tasks.map((task) => [task.id, task]));
  const lastMessage = new Map<string, string>();
  for (const message of state.messages) lastMessage.set(message.task_id, message.id);
  for (const message of state.messages) {
    if (message.content) nodes.push(messageView(message));
    if (lastMessage.get(message.task_id) === message.id) {
      const task = tasks.get(message.task_id);
      if (task && ['failed', 'interrupted', 'stopped'].includes(task.status)) {
        nodes.push(element('p', 'outcome', task.status === 'stopped'
          ? 'Stopped. Any files already created are still available.' : task.detail));
      }
    }
  }
  const active = state.tasks.find((task) => task.status === 'running');
  if (active) {
    const working = element('div', 'working');
    working.setAttribute('role', 'status');
    working.append(element('span', 'pulse'), element('span', '', active.detail || 'Working on it…'));
    nodes.push(working);
  }
  const queued = state.tasks.filter((task) => task.status === 'queued');
  for (const task of queued) nodes.push(queuedView(task));
  container.replaceChildren(...nodes);
  if (atEnd) container.scrollTop = container.scrollHeight;
}

function queuedView(task: Task): HTMLElement {
  const row = element('div', 'queued');
  const button = element('button', 'text-button', 'Cancel');
  button.dataset.stopTask = task.id;
  row.append(element('span', '', 'I’ll get to this next.'), button);
  return row;
}

function messageView(message: ChatMessage): HTMLElement {
  const row = element('article', `message ${message.role}`);
  row.dataset.messageId = message.id;
  const content = element('div', 'message-content', message.content);
  if (message.role === 'assistant') row.append(element('div', 'speaker', 'Ayati'));
  row.append(content);
  return row;
}

export function renderFiles(container: HTMLElement, files: Artifact[]): void {
  const visible = files.filter((file) => file.kind !== 'log');
  const nodes: HTMLElement[] = [];
  if (!visible.length) nodes.push(element('p', 'empty-files', 'Files you share and files Ayati creates will appear here.'));
  for (const file of visible) {
    const link = element('a', 'file-row');
    link.href = `/api/files/${encodeURIComponent(file.id)}`;
    link.setAttribute('download', file.name);
    const info = element('div', 'file-info');
    info.append(element('span', 'file-name', file.name), element('span', 'file-meta',
      `${file.kind === 'upload' ? 'You shared' : 'Ayati created'} · ${formatSize(file.size)}`));
    link.append(element('span', 'file-icon', '↧'), info);
    nodes.push(link);
  }
  container.replaceChildren(...nodes);
}

function formatSize(bytes: number): string {
  return bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024
    ? `${Math.ceil(bytes / 1024)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
