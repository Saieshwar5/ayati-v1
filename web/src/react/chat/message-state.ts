import type { Message } from './model';

/** Stream events and snapshots may cross in flight; revisions order their content. */
export function newerMessage(current: Message, incoming: Message): Message {
  if ((current.revision ?? 0) > (incoming.revision ?? 0)) return current;
  if ((current.revision ?? 0) === (incoming.revision ?? 0)) {
    if (current.state !== 'queued' && incoming.state === 'queued') return current;
    return { ...incoming, delivered: Math.max(current.delivered ?? 0, incoming.delivered ?? 0), seen: Math.max(current.seen ?? 0, incoming.seen ?? 0) };
  }
  return incoming;
}

export function mergeMessages(...groups: Message[][]): Message[] {
  const messages = new Map<string, Message>();
  for (const group of groups) for (const message of group) {
    const current = messages.get(message.id);
    messages.set(message.id, current ? newerMessage(current, message) : message);
  }
  return [...messages.values()].sort((a, b) => (a.sequence ?? a.at) - (b.sequence ?? b.at));
}
