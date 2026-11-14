import { useEffect, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest, conversationName, type Conversation, type Directory } from './model';
import { workspaceSession } from '../../workspace-context';
import { FILTER_LABELS, noFilters, parseSearch, rememberSearch, withoutFilter, type Filters } from './search-query';
import './message-search.css';

export type MessageHit = {
  id: string; chat: string; chat_kind: string; chat_name: string; author: string; kind: 'user' | 'agent';
  name: string | null; at: number; thread: string | null; has_files: boolean; snippet: string;
};
type Page = { results: MessageHit[]; terms: string[]; next_before: number | null };
type State = { query: string; hits: MessageHit[]; terms: string[]; next: number | null; loading: boolean; error: string };

const WAIT_MS = 250;
export const searchable = (query: string) => query.trim().length >= 2;

/** What is searched: words and filters, as the server takes them. */
type Asked = { q: string; chat?: string; from?: string } & Partial<Record<'files' | 'mentions_me' | 'agents' | 'links' | 'threads', boolean>>;

/** Messages matching the words and filters, newest first. A slower answer for
 * an older query never replaces the current one. */
function useMessageSearch(asked: Asked) {
  const [state, setState] = useState<State>({ query: '', hits: [], terms: [], next: null, loading: false, error: '' });
  // One string for the whole request, so any change of words or filters reads again.
  const text = searchable(asked.q) ? params(asked) : '';
  useEffect(() => {
    if (!text) { setState({ query: '', hits: [], terms: [], next: null, loading: false, error: '' }); return; }
    const request = new AbortController();
    setState(previous => ({ ...previous, loading: true, error: '' }));
    const timer = window.setTimeout(() => {
      void chatRequest<Page>(`/message-search?${text}`, undefined, request.signal)
        .then(page => { if (!request.signal.aborted) setState({ query: text, hits: page.results, terms: page.terms, next: page.next_before, loading: false, error: '' }); })
        .catch(error => { if (!request.signal.aborted) setState(previous => ({ ...previous, loading: false, error: errorMessage(error) })); });
    }, WAIT_MS);
    return () => { request.abort(); window.clearTimeout(timer); };
  }, [text]);
  const more = async () => {
    if (state.next === null || state.loading) return;
    const asked = state.query;
    setState(previous => ({ ...previous, loading: true }));
    try {
      const page = await chatRequest<Page>(`/message-search?${asked}&before=${state.next}`);
      setState(previous => previous.query !== asked ? previous
        : { ...previous, hits: [...previous.hits, ...page.results], next: page.next_before, loading: false });
    } catch (error) {
      setState(previous => ({ ...previous, loading: false, error: errorMessage(error) }));
    }
  };
  return { ...state, current: state.query === text, more };
}

function params({ q, ...rest }: Asked) {
  const search = new URLSearchParams({ q: q.trim() });
  for (const [key, value] of Object.entries(rest)) if (value) search.set(key, String(value));
  return search.toString();
}

/** The snippet with each word that starts with a searched term marked. */
export function highlight(text: string, terms: string[]): ReactNode[] {
  if (!terms.length) return [text];
  const escaped = terms.map(term => term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'));
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}])((?:${escaped.join('|')})[\\p{L}\\p{M}\\p{N}]*)`, 'giu');
  return text.split(pattern).map((part, index) => index % 2 ? <mark key={index}>{part}</mark> : part);
}

const when = (seconds: number) => new Date(seconds * 1000).toLocaleString([], { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });

/** Search results with filter chips; choosing one opens that message in its
 * conversation and remembers the search. `setQuery` lets a written filter's
 * chip remove it from the query. */
export function MessageResults({ query, chat, chats, directory, open, setQuery }: {
  query: string; chat?: string; chats: Conversation[]; directory: Directory; open: (chat: string, message: string) => void;
  setQuery?: (query: string) => void;
}) {
  const [filters, setFilters] = useState<Filters>(noFilters);
  const me = workspaceSession?.user?.id;
  const parsed = parseSearch(query, { chats, directory, me });
  const search = useMessageSearch({
    q: parsed.words, chat: parsed.chat?.id ?? chat, from: filters.mine ? me : parsed.from?.id,
    files: filters.files || parsed.files, links: filters.links || parsed.links,
    mentions_me: filters.mentions, agents: filters.agents, threads: filters.threads,
  });
  const written = [
    ...(parsed.chat && !chat ? [{ key: 'in' as const, label: `in: ${parsed.chat.label}` }] : []),
    ...(parsed.from && !filters.mine ? [{ key: 'from' as const, label: `from: ${parsed.from.label}` }] : []),
    ...(parsed.files ? [{ key: 'has' as const, value: 'file', label: 'has: file' }] : []),
    ...(parsed.links ? [{ key: 'has' as const, value: 'link', label: 'has: link' }] : []),
  ];
  const chips = <div className="chat-search-chips" role="group" aria-label="Filter messages">
    {written.map(item => <span className="chat-search-chip written" key={item.label}>{item.label}
      {setQuery && <button type="button" aria-label={`Remove ${item.label}`} onClick={() => setQuery(withoutFilter(query, item.key, 'value' in item ? item.value : undefined))}>×</button>}</span>)}
    {FILTER_LABELS.map(([key, label]) => <Button key={key} className="chat-search-chip" aria-pressed={filters[key]} onClick={() => setFilters(previous => ({ ...previous, [key]: !previous[key] }))}>{label}</Button>)}
  </div>;
  const choose = (hit: MessageHit) => { rememberSearch(query); open(hit.chat, hit.id); };
  if (!searchable(parsed.words)) return <>{chips}
    {parsed.unknown.length > 0 && <p className="chat-error" role="alert">Nothing matches {parsed.unknown.join(', ')}.</p>}
    <p className="ui-help">Type at least two letters to search messages. Filter by writing <b>in:</b>, <b>from:</b> or <b>has:</b>.</p></>;
  const where = (hit: MessageHit) => {
    const conversation = chats.find(c => c.id === hit.chat);
    return conversation ? (conversation.kind === 'direct' ? conversationName(conversation, directory) : `#${conversation.name}`) : hit.chat_name;
  };
  return <section className="chat-message-search" aria-label="Messages" aria-busy={search.loading}>
    {chips}
    {parsed.unknown.length > 0 && <p className="chat-error" role="alert">Nothing matches {parsed.unknown.join(', ')}; searching without it.</p>}
    <p className="ui-help" role="status">
      {search.error ? '' : search.loading && !search.current ? 'Searching messages…'
        : search.hits.length ? `${search.hits.length}${search.next !== null ? '+' : ''} ${search.hits.length === 1 ? 'message' : 'messages'}` : 'No messages match.'}
    </p>
    {search.error && <p className="chat-error" role="alert">{search.error}</p>}
    <ul className="chat-message-hits">
      {search.hits.map(hit => <li key={hit.id}>
        <button type="button" onClick={() => choose(hit)} aria-label={`Open message from ${hit.name ?? 'someone'} in ${where(hit)}`}>
          <span className="chat-hit-meta">
            <strong>{hit.name ?? 'Former member'}{hit.kind === 'agent' && <span className="chat-hit-agent">AI</span>}</strong>
            {!chat && <span>{where(hit)}</span>}
            {hit.thread && <span>in a thread</span>}
            {hit.has_files && <span>has files</span>}
            <time dateTime={new Date(hit.at * 1000).toISOString()}>{when(hit.at)}</time>
          </span>
          <span className="chat-hit-text">{highlight(hit.snippet, search.terms)}</span>
        </button>
      </li>)}
    </ul>
    {search.next !== null && search.current && <Button disabled={search.loading} onClick={() => void search.more()}>{search.loading ? 'Loading…' : 'Older messages'}</Button>}
  </section>;
}
