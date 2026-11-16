import { decodeMentions, mentionedAgents } from './mentions';
import { AgentFace, PersonFace } from '../faces';
import type { Directory, Message } from './model';

/** Who a thread's plain reply goes to: the agent a person last asked in it
 * (by mention, or the agent that started it), while it is still here. Agents
 * asked by another agent as helpers never take over (D246 hand-offs). */
export function continuingAgent(root: Message, replies: Message[], present: string[]): string | undefined {
  let agent = root.kind === 'agent' ? root.author : undefined;
  for (const message of [root, ...replies]) {
    if (message.kind !== 'user' || message.state === 'deleted') continue;
    const asked = mentionedAgents(message.content);
    if (asked.length) agent = asked[asked.length - 1];
  }
  return agent && present.includes(agent) ? agent : undefined;
}

/** Whether a draft names anyone: then it goes only to whom it names. */
export const namesSomeone = (text: string) => decodeMentions(text).mentions.length > 0;

/** A thread at a glance, from the replies loaded with the conversation. */
export function threadSummary(messages: Message[], root: string) {
  const replies = messages.filter(message => message.thread === root && message.state !== 'deleted');
  const shown = replies.filter(message => message.kind !== 'agent' || message.content || message.state !== 'queued');
  const people: { kind: 'user' | 'agent'; id: string }[] = [];
  for (const message of shown) {
    const kind = message.kind === 'agent' ? 'agent' : 'user';
    if (!people.some(p => p.id === message.author)) people.push({ kind, id: message.author });
  }
  return {
    count: shown.length,
    people,
    last: shown.length ? shown[shown.length - 1].at : undefined,
    working: [...new Set(replies.filter(m => m.kind === 'agent' && m.state === 'queued').map(m => m.author))],
  };
}

const ago = (at: number) => {
  const minutes = Math.max(0, Math.round((Date.now() / 1000 - at) / 60));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours} h ago` : new Date(at * 1000).toLocaleDateString([], { day: 'numeric', month: 'short' });
};

/** Under a message with a thread: faces, count, when the last reply came and
 * which agent is working there. Opens the thread. */
export function ThreadLine({ summary, directory, open }: { summary: ReturnType<typeof threadSummary>; directory: Directory; open?: () => void }) {
  const { count, people, last, working } = summary;
  if (!count && !working.length) return null;
  const name = (id: string) => directory.agents.find(agent => agent.id === id)?.name ?? 'Agent';
  const replies = `${count} ${count === 1 ? 'reply' : 'replies'}`;
  const busy = working.length ? `${working.map(name).join(' and ')} ${working.length === 1 ? 'is' : 'are'} working…` : '';
  const label = [count ? replies : '', busy, count && last ? `last reply ${ago(last)}` : ''].filter(Boolean).join(' · ');
  return <button type="button" className="chat-thread-link chat-thread-count chat-thread-line" aria-label={`Open thread: ${label}`} title="Open thread" onClick={open}>
    <span className="chat-thread-faces" aria-hidden="true">{people.slice(0, 3).map(person => person.kind === 'agent'
      ? <AgentFace key={person.id} agent={directory.agents.find(a => a.id === person.id) ?? { id: person.id, name: name(person.id) }} className="chat-thread-face" />
      : <PersonFace key={person.id} person={directory.users.find(u => u.id === person.id) ?? { id: person.id, name: 'Former member' }} className="chat-thread-face" />)}</span>
    {count > 0 && <strong>{replies}</strong>}
    {busy && <span className="chat-thread-working">{busy}</span>}
    {count > 0 && last !== undefined && <span className="chat-thread-last">last reply {ago(last)}</span>}
  </button>;
}
