import { useEffect, useState, type ReactNode } from 'react';
import { Button } from '../ui';
import { errorMessage } from '../requests';
import { chatRequest, conversationName, type Conversation, type Directory } from './model';
import './message-search.css';

export type MessageHit = {
  id: string; chat: string; chat_kind: string; chat_name: string; author: string; kind: 'user' | 'agent';
  name: string | null; at: number; thread: string | null; has_files: boolean; snippet: string;
};
type Page = { results: MessageHit[]; terms: string[]; next_before: number | null };
type State = { query: string; hits: MessageHit[]; terms: string[]; next: number | null; loading: boolean; error: string };

const WAIT_MS = 250;
export const searchable = (query: string) => query.trim().length >= 2;

/** Messages matching `query` (in `chat` only, if given), newest first. A slower
 * answer for an older query never replaces the current one. */
function useMessageSearch(query: string, chat?: string) {
  const [state, setState] = useState<State>({ query: '', hits: [], terms: [], next: null, loading: false, error: '' });
  const text = query.trim();
  useEffect(() => {
    if (!searchable(text)) { setState({ query: '', hits: [], terms: [], next: null, loading: false, error: '' }); return; }
    const request = new AbortController();
    setState(previous => ({ ...previous, loading: true, error: '' }));
    const timer = window.setTimeout(() => {
      void chatRequest<Page>(`/message-search?${params(text, chat)}`, undefined, request.signal)
        .then(page => { if (!request.signal.aborted) setState({ query: text, hits: page.results, terms: page.terms, next: page.next_before, loading: false, error: '' }); })
        .catch(error => { if (!request.signal.aborted) setState(previous => ({ ...previous, loading: false, error: errorMessage(error) })); });
    }, WAIT_MS);
    return () => { request.abort(); window.clearTimeout(timer); };
  }, [text, chat]);
  const more = async () => {
    if (state.next === null || state.loading) return;
    const asked = state.query;
    setState(previous => ({ ...previous, loading: true }));
    try {
      const page = await chatRequest<Page>(`/message-search?${params(asked, chat, state.next)}`);
      setState(previous => previous.query !== asked ? previous
        : { ...previous, hits: [...previous.hits, ...page.results], next: page.next_before, loading: false });
    } catch (error) {
      setState(previous => ({ ...previous, loading: false, error: errorMessage(error) }));
    }
  };
  return { ...state, current: state.query === text, more };
}

function params(q: string, chat?: string, before?: number) {
  const search = new URLSearchParams({ q });
  if (chat) search.set('chat', chat);
  if (before !== undefined) search.set('before', String(before));
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

/** Search results; choosing one opens that message in its conversation. */
export function MessageResults({ query, chat, chats, directory, open }: {
  query: string; chat?: string; chats: Conversation[]; directory: Directory; open: (chat: string, message: string) => void;
}) {
  const search = useMessageSearch(query, chat);
  if (!searchable(query)) return <p className="ui-help">Type at least two letters to search messages.</p>;
  const where = (hit: MessageHit) => {
    const conversation = chats.find(c => c.id === hit.chat);
    return conversation ? (conversation.kind === 'direct' ? conversationName(conversation, directory) : `#${conversation.name}`) : hit.chat_name;
  };
  return <section className="chat-message-search" aria-label="Messages" aria-busy={search.loading}>
    <p className="ui-help" role="status">
      {search.error ? '' : search.loading && !search.current ? 'Searching messages…'
        : search.hits.length ? `${search.hits.length}${search.next !== null ? '+' : ''} ${search.hits.length === 1 ? 'message' : 'messages'}` : 'No messages match.'}
    </p>
    {search.error && <p className="chat-error" role="alert">{search.error}</p>}
    <ul className="chat-message-hits">
      {search.hits.map(hit => <li key={hit.id}>
        <button type="button" onClick={() => open(hit.chat, hit.id)} aria-label={`Open message from ${hit.name ?? 'someone'} in ${where(hit)}`}>
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
