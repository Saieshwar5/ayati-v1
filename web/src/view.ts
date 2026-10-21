import { messageHtml } from './markdown';
import { workspaceUrl } from './workspace-context.ts';
import type { Artifact, ChatMessage, Snapshot, Task } from './types';

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K, className = '', text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

export function renderConversation(container: HTMLElement, state: Snapshot, agentName = 'Relay'): void {
  const atEnd = container.scrollHeight - container.scrollTop - container.clientHeight < 100;
  const nodes: HTMLElement[] = [];
  if (!state.messages.length) {
    const welcome = element('section', 'welcome');
    const mark = element('span', 'relay-brand relay-symbol'); mark.setAttribute('aria-hidden', 'true');
    welcome.append(mark,
      element('h1', '', 'A little help, whenever you need it.'),
      element('p', '', 'Ask a question, work through an idea, or share a file.'));
    nodes.push(welcome);
  }
  const tasks = new Map(state.tasks.map((task) => [task.id, task]));
  const lastMessage = new Map<string, string>();
  for (const message of state.messages) lastMessage.set(message.task_id, message.id);
  for (const message of state.messages) {
    if (message.content) nodes.push(messageView(message, agentName));
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
  for (const permission of state.permissions ?? []) {
    const card = element('section', 'permission');
    card.setAttribute('aria-label', 'Action approval');
    card.append(element('strong', '', permission.title));
    const details = element('pre', 'permission-details', JSON.stringify(permission.action, null, 2));
    card.append(details);
    for (const [label, decision] of [['Allow once', 'allow'], ['Deny', 'deny']]) {
      const button = element('button', 'text-button', label);
      button.dataset.permission = permission.id;
      button.dataset.decision = decision;
      card.append(button);
    }
    nodes.push(card);
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

function messageView(message: ChatMessage, agentName: string): HTMLElement {
  const row = element('article', `message ${message.role}`);
  row.dataset.messageId = message.id;
  let body = message.content;
  const attachments: string[] = [];
  const marker = '\n\n<ayati-attachments>\n';
  const start = body.lastIndexOf(marker);
  if (message.role === 'user' && start >= 0 && body.endsWith('\n</ayati-attachments>')) {
    try {
      const files = JSON.parse(body.slice(start + marker.length, -'\n</ayati-attachments>'.length));
      if (Array.isArray(files) && files.every(f => typeof f.name === 'string' && typeof f.id === 'string')) {
        attachments.push(...files.map(f => f.name)); body = body.slice(0, start);
      }
    } catch { /* Preserve ordinary messages that happen to include the marker. */ }
  }
  const content = element('div', 'message-content message-markdown');
  content.innerHTML = messageHtml(body);
  for (const name of attachments) content.append(element('span', 'sent-attachment', `↧ ${name}`));
  if (message.role === 'assistant') row.append(element('div', 'speaker', agentName));
  row.append(content);
  return row;
}

export function renderFiles(container: HTMLElement, files: Artifact[], agent = 'local-agent'): void {
  const visible = files.filter((file) => file.kind !== 'log');
  const nodes: HTMLElement[] = [];
  if (!visible.length) nodes.push(element('p', 'empty-files', 'Files you share and files Relay creates will appear here.'));
  for (const file of visible) {
    const link = element('a', 'file-row');
    link.href = workspaceUrl(`/api/files/${encodeURIComponent(file.id)}?agent=${encodeURIComponent(agent)}`);
    link.setAttribute('download', file.name);
    const info = element('div', 'file-info');
    info.append(element('span', 'file-name', file.name), element('span', 'file-meta',
      `${file.kind === 'upload' ? 'You shared' : 'Relay created'} · ${formatSize(file.size)}`));
    link.append(element('span', 'file-icon', '↧'), info);
    nodes.push(link);
  }
  container.replaceChildren(...nodes);
}

function formatSize(bytes: number): string {
  return bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024
    ? `${Math.ceil(bytes / 1024)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
