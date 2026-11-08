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

/** A handoff is at most three levels deep; each level reads just after the one it asked. */
const handoffLevels = 3;
const afterHelper = 0.25;

/** A reply is created when the request is sent, so it sorts before anything the
 * agent says while working on it ("got it", a milestone) and before an exchange it
 * starts with another agent (its handoff message and that agent's answer, D246).
 * It reads after them, so it takes the position of the latest of them in the same
 * thread; after a helper's answer it comes just after that answer. */
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
  const orders = new Map<string, number>();
  const order = (message: Message, level = 0): number => {
    const known = orders.get(message.id);
    if (known !== undefined) return known;
    let value = Math.max(position(message), latest.get(message.id) ?? 0);
    // Only an exchange beside this reply moves it; one in a thread under it does not.
    for (const handoff of level < handoffLevels ? message.coordination?.outgoing ?? [] : []) {
      if ((handoff.thread ?? null) !== (message.thread ?? null)) continue;
      const request = messages.get(handoff.request_message), helper = messages.get(handoff.child);
      if (request) value = Math.max(value, position(request));
      if (helper) value = Math.max(value, order(helper, level + 1) + afterHelper);
    }
    orders.set(message.id, value);
    return value;
  };
  // At the same position the update comes first, then the reply it belongs to.
  return [...messages.values()].sort((a, b) => order(a) - order(b) || (a.update_of ? 0 : 1) - (b.update_of ? 0 : 1));
}
