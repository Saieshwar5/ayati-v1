import type { AgentEvent, Snapshot } from './types';

export function mergeSnapshot(local: Snapshot, incoming: Snapshot): Snapshot {
  for (const message of incoming.messages) {
    const current = local.messages.find((m) => m.id === message.id);
    if (message.state === 'streaming' && current && current.content.length > message.content.length) {
      message.content = current.content;
    }
  }
  return incoming;
}

// UTF-16 offsets match JavaScript string indices and survive snapshot/delta races.
// Return false for a gap so the caller reloads persisted state rather than inventing text.
export function applyDelta(state: Snapshot, update: AgentEvent): boolean {
  if (!update.message_id || !update.task_id || update.text === undefined || update.offset === undefined) return false;
  let message = state.messages.find((m) => m.id === update.message_id);
  if (!message) {
    if (update.offset !== 0) return false;
    message = { id: update.message_id, task_id: update.task_id, role: 'assistant',
      content: '', state: 'streaming', created_at: new Date().toISOString() };
    state.messages.push(message);
  }
  if (update.offset > message.content.length) return false;
  if (update.offset + update.text.length > message.content.length) {
    message.content = message.content.slice(0, update.offset) + update.text;
  }
  return true;
}
