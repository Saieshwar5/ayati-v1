// What a typed message search asks for: its words, plus filters written as
// `in:#orders`, `from:Meera` (or `from:me`), `has:file` and `has:link`, and
// the searches a person ran recently on this device.
import { storageScope } from '../../workspace-context';
import { displayAgentName } from '../../agents';
import { conversationName, type Conversation, type Directory } from './model';

export type Filters = { mine: boolean; mentions: boolean; agents: boolean; links: boolean; files: boolean; threads: boolean };
export const noFilters: Filters = { mine: false, mentions: false, agents: false, links: false, files: false, threads: false };
export const FILTER_LABELS: [keyof Filters, string][] = [
  ['mine', 'From me'], ['mentions', 'Mentions me'], ['agents', 'From an agent'],
  ['links', 'Has a link'], ['files', 'Has a file'], ['threads', 'In threads'],
];

export type ParsedSearch = {
  words: string;
  /** `in:` — the conversation id and how it reads. */
  chat?: { id: string; label: string };
  /** `from:` — a person's or agent's id and name. */
  from?: { id: string; label: string };
  files: boolean; links: boolean;
  /** Written filters that name nothing this person can see. */
  unknown: string[];
};

const norm = (value: string) => value.toLocaleLowerCase().replace(/^[#@]/, '');

export function parseSearch(text: string, { chats, directory, me }: { chats: Conversation[]; directory: Directory; me?: string }): ParsedSearch {
  const result: ParsedSearch = { words: '', files: false, links: false, unknown: [] };
  const words: string[] = [];
  for (const token of text.split(/\s+/).filter(Boolean)) {
    const match = /^(in|from|has):(.+)$/i.exec(token);
    if (!match) { words.push(token); continue; }
    const [, key, raw] = match, value = norm(raw);
    if (key.toLowerCase() === 'has') {
      if (['file', 'files'].includes(value)) result.files = true;
      else if (['link', 'links'].includes(value)) result.links = true;
      else result.unknown.push(token);
    } else if (key.toLowerCase() === 'in') {
      const chat = chats.find(c => norm(conversationName(c, directory)) === value)
        ?? chats.find(c => norm(conversationName(c, directory)).startsWith(value));
      if (chat) result.chat = { id: chat.id, label: chat.kind === 'direct' ? conversationName(chat, directory) : `#${chat.name}` };
      else result.unknown.push(token);
    } else {
      const people = [
        ...directory.users.map(user => ({ id: user.id, name: user.name })),
        ...directory.agents.map(agent => ({ id: agent.id, name: displayAgentName(agent) })),
      ];
      const who = value === 'me' && me ? people.find(p => p.id === me) ?? { id: me, name: 'me' }
        : people.find(p => norm(p.name) === value) ?? people.find(p => norm(p.name).split(/\s+/).some(part => part.startsWith(value)));
      if (who) result.from = { id: who.id, label: who.id === me ? 'me' : who.name };
      else result.unknown.push(token);
    }
  }
  result.words = words.join(' ');
  return result;
}

/** The query without one written filter, for the × on its chip. */
export const withoutFilter = (text: string, key: 'in' | 'from' | 'has', value?: string) =>
  text.split(/\s+/).filter(token => {
    const match = /^(in|from|has):(.+)$/i.exec(token);
    return !match || match[1].toLowerCase() !== key || (value !== undefined && norm(match[2]) !== value);
  }).join(' ');

const RECENT = 5;
const recentKey = () => storageScope('relay.chat.recent-searches');
export function recentSearches(): string[] {
  try {
    const saved = JSON.parse(localStorage.getItem(recentKey()) ?? '[]');
    return Array.isArray(saved) ? saved.filter((item): item is string => typeof item === 'string').slice(0, RECENT) : [];
  } catch { return []; }
}
/** Kept on this device for this person and workspace; an unavailable store keeps nothing. */
export function rememberSearch(text: string) {
  const query = text.trim();
  if (!query) return;
  try { localStorage.setItem(recentKey(), JSON.stringify([query, ...recentSearches().filter(item => item !== query)].slice(0, RECENT))); } catch { /* Not remembered. */ }
}
export function clearSearches() { try { localStorage.removeItem(recentKey()); } catch { /* Nothing to clear. */ } }
