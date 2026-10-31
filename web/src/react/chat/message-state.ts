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

/** A reply is created when the request is sent, so it sorts before anything the
 * agent says while working on it ("got it", a milestone). It reads after them,
 * so it takes the position of its latest update in the same thread. */
export function mergeMessages(...groups: Message[][]): Message[] {
  const messages = new Map<string, Message>();
  for (const group of groups) for (const message of group) {
    const current = messages.get(message.id);
    messages.set(message.id, current ? newerMessage(current, message) : message);
  }
  const position = (message: Message) => message.sequence ?? message.at;
  const latest = new Map<string, number>();
  for (const message of messages.values()) {
    if (!message.update_of) continue;
    const reply = messages.get(message.update_of);
    if (reply && (reply.thread ?? null) === (message.thread ?? null)) latest.set(reply.id, Math.max(latest.get(reply.id) ?? 0, position(message)));
  }
  const order = (message: Message) => Math.max(position(message), latest.get(message.id) ?? 0);
  // At the same position the update comes first, then the reply it belongs to.
  return [...messages.values()].sort((a, b) => order(a) - order(b) || (a.update_of ? 0 : 1) - (b.update_of ? 0 : 1));
}
