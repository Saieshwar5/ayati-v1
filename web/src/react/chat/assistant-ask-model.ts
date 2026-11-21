import { storageScope } from '../../workspace-context';
import { decodeMentions, encodeMentions, type Mention } from './mentions';
import type { Message } from './model';

/** A reply your assistant is writing, or has written, for one conversation or
 * thread. Only its person ever has this. */
export type Asked = {
  /** The assistant's reply that holds the draft. */
  reply: string;
  state: 'writing' | 'ready' | 'failed';
  /** What you asked, and the last change you asked for. */
  instruction: string; change?: string;
  text: string;
  /** The newest message here when you asked, to tell when the conversation moved on. */
  sequence: number;
  at: number;
  /** The message your reply quotes, and the one the draft answers. */
  quote?: string; answering?: string;
};

const mentionName = (assistant: Mention) => assistant.name.replace(/[\]\n]/g, ' ');

/** What a message box asks your own assistant: its text without the
 * assistant's mention, other mentions as plain names. Undefined when the text
 * does not mention your assistant, so it is an ordinary message. */
export function assistantRequest(source: string, assistant: string | undefined): string | undefined {
  if (!assistant) return undefined;
  const { text, mentions } = decodeMentions(source);
  const own = mentions.filter(mention => mention.kind === 'agent' && mention.id === assistant);
  if (!own.length) return undefined;
  let rest = '', at = 0;
  for (const mention of own) { rest += text.slice(at, mention.start); at = mention.end; }
  return (rest + text.slice(at)).replace(/[ \t]{2,}/g, ' ').trim();
}

/** Nothing asked beyond "answer": the draft replies to the newest message meant for you. */
export const answersLatest = (instruction: string) =>
  /^((please\s+)?(respond|reply|answer)(\s+(please|to\s+(this|that|it|him|her|them)))?[.!]?)?$/i.test(instruction.trim());

/** The newest message someone else meant for this person: in a direct chat
 * their latest, elsewhere the latest addressed to the person, else the latest. */
export function latestForYou(messages: Message[], viewer: string | undefined, direct: boolean): Message | undefined {
  const others = messages.filter(message => message.author !== viewer && ['sent', 'complete'].includes(message.state) && message.content.trim()).reverse();
  if (direct) return others[0];
  return others.find(message => message.for_me || decodeMentions(message.content).mentions.some(mention => mention.kind === 'user' && mention.id === viewer)) ?? others[0];
}

/** The draft with your assistant mentioned first, ready for what to ask. */
export function mentionAssistant(source: string, assistant: Mention): string {
  const { text, mentions } = decodeMentions(source);
  const name = mentionName(assistant), lead = `@${name} `;
  return encodeMentions(lead + text, [{ ...assistant, name, start: 0, end: lead.length - 1 },
    ...mentions.map(mention => ({ ...mention, start: mention.start + lead.length, end: mention.end + lead.length }))]);
}

/** Where plain text opens by naming your assistant without mentioning it. Sent
 * as written, what was meant for the assistant would be posted. */
function typedName(source: string, assistant: Mention): { start: number; end: number } | undefined {
  const { text, mentions } = decodeMentions(source);
  const lead = `@${mentionName(assistant)}`, start = text.length - text.trimStart().length, end = start + lead.length;
  if (text.slice(start, end).toLocaleLowerCase() !== lead.toLocaleLowerCase()) return undefined;
  if (end < text.length && !/[\s,.:;!?]/.test(text[end])) return undefined;
  return mentions.some(mention => mention.start < end && mention.end > start) ? undefined : { start, end };
}
export const namesAssistant = (source: string, assistant: Mention) => Boolean(typedName(source, assistant));

/** That typed name as the mention it was meant to be. */
export function mentionTyped(source: string, assistant: Mention): string {
  const typed = typedName(source, assistant);
  if (!typed) return source;
  const { text, mentions } = decodeMentions(source), name = mentionName(assistant);
  return encodeMentions(`${text.slice(0, typed.start)}@${name}${text.slice(typed.end)}`, [...mentions, { ...assistant, name, ...typed }]);
}

const hour = 3_600_000;
const key = (chat: string, thread?: string) => storageScope(`relay.chat.ask.${chat}${thread ? `.${thread}` : ''}`);

/** The draft asked for here earlier in this browser tab, by this person in this workspace. */
export function loadAsked(chat: string, thread?: string): Asked | undefined {
  try {
    const kept = JSON.parse(sessionStorage.getItem(key(chat, thread)) ?? 'null') as Asked | null;
    return kept && typeof kept.reply === 'string' && Date.now() - kept.at < hour ? kept : undefined;
  } catch { return undefined; }
}
export function keepAsked(chat: string, thread: string | undefined, asked: Asked | undefined) {
  try {
    if (asked) sessionStorage.setItem(key(chat, thread), JSON.stringify(asked)); else sessionStorage.removeItem(key(chat, thread));
  } catch { /* The card still works for this visit. */ }
}
