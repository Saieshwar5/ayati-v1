import { alertText } from './notifications';
import type { Message } from './model';

/** A thread in this conversation with replies the viewer has not read. */
export type ThreadWithNew = { root: Message; fresh: number; forMe: boolean };

/** Threads with new replies, the one with the latest new reply first. A reply
 * whose question is not loaded cannot be opened here and is left out. */
export function threadsWithNew(messages: Message[], isNew: (message: Message) => boolean): ThreadWithNew[] {
  const roots = new Map(messages.filter(message => !message.thread).map(message => [message.id, message]));
  const threads = new Map<string, ThreadWithNew & { latest: number }>();
  for (const message of messages) {
    const root = message.thread ? roots.get(message.thread) : undefined;
    if (!root || message.state === 'deleted' || !isNew(message)) continue;
    const entry = threads.get(root.id) ?? { root, fresh: 0, forMe: false, latest: 0 };
    entry.fresh++; entry.forMe ||= Boolean(message.for_me); entry.latest = Math.max(entry.latest, message.sequence ?? message.at);
    threads.set(root.id, entry);
  }
  return [...threads.values()].sort((a, b) => b.latest - a.latest).map(({ latest: _latest, ...thread }) => thread);
}

const SHOWN = 3;

/** Above the messages: threads with replies not read yet. Those replies keep the
 * conversation's unread count until their thread is opened, and their question
 * may be far up or above the loaded history, so each can be opened from here. */
export function NewRepliesBar({ threads, open }: { threads: ThreadWithNew[]; open: (root: string) => void }) {
  if (!threads.length) return null;
  const more = threads.length - SHOWN;
  return <div className="chat-new-replies" role="region" aria-label="Threads with new replies">
    <span className="chat-new-replies-title">New replies in {threads.length} {threads.length === 1 ? 'thread' : 'threads'}</span>
    {threads.slice(0, SHOWN).map(({ root, fresh, forMe }) => {
      const words = alertText(root.content) || 'A message';
      return <button type="button" key={root.id} className="chat-new-replies-thread" onClick={() => open(root.id)}
        aria-label={`Open thread “${words}”: ${fresh} new ${fresh === 1 ? 'reply' : 'replies'}${forMe ? ', for you' : ''}`}>
        <span className="chat-new-replies-words">{words}</span>
        <span className={`chat-thread-new${forMe ? ' chat-thread-new-mention' : ''}`} aria-hidden="true">{fresh} new</span>
      </button>;
    })}
    {more > 0 && <span className="chat-new-replies-more">and {more} more</span>}
  </div>;
}
