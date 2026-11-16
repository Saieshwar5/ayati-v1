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
  return <div className="chat-thread-follow">
    <span>{state.following ? 'You get alerts for new replies here.' : 'You don’t get alerts for this thread.'}</span>
    <Button disabled={mutation.busy} onClick={change}>{state.following ? 'Stop following' : 'Follow'}</Button>
    {mutation.status && <span role="alert">{mutation.status}</span>}
  </div>;
}
