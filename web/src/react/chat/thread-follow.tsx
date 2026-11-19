import { useEffect, useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { chatRequest } from './model';

type Following = { following: boolean; chosen: boolean };

/** Follow or stop following a thread. People follow threads they started,
 * wrote in, were mentioned in or asked an agent in; replies there alert them. */
export function ThreadFollow({ chat, thread }: { chat: string; thread: string }) {
  const [state, setState] = useState<Following>();
  const mutation = useMutation();
  const path = `/chats/${chat}/threads/${thread}/follow`;
  useEffect(() => {
    const abort = new AbortController();
    setState(undefined);
    chatRequest<Following>(path, undefined, abort.signal)
      .then(next => { if (!abort.signal.aborted) setState(next); })
      .catch(() => { /* Without the state the button stays hidden; following still works. */ });
    return () => abort.abort();
  }, [path]);
  if (!state) return null;
  const change = () => void mutation.run(async () => {
    setState(await chatRequest<Following>(path, { following: !state.following }));
  });
  // The label is the state; the sentence stays available as the button's description.
  const about = state.following ? 'You get alerts for new replies here.' : 'You don’t get alerts for this thread.';
  return <span className="chat-thread-follow">
    <Button aria-pressed={state.following} title={about} disabled={mutation.busy} onClick={change}><BellIcon />{state.following ? 'Following' : 'Follow'}</Button>
    {mutation.status && <span role="alert">{mutation.status}</span>}
  </span>;
}

function BellIcon() {
  return <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M6 9a6 6 0 0 1 12 0c0 6 2.5 7 2.5 7h-17S6 15 6 9ZM10 20a2 2 0 0 0 4 0" /></svg>;
}
