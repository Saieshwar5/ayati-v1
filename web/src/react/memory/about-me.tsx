import { useCallback } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { day, forgetAboutMe, kindName, loadAboutMe } from './memory-model';
import './memory.css';

/** What a shared agent remembers about you from your own conversation with it.
 * Only you see this; it is used only when you ask it there. */
export function AboutMe({ agent, name }: { agent: string; name: string }) {
  const { data, error, refresh } = useResource(useCallback(() => loadAboutMe(agent), [agent]));
  const mutation = useMutation();
  if (error && !data) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading what {name} remembers about you…</p>;
  if (!data.available) return <p className="ui-help">Memory is not available in this workspace yet.</p>;
  return <section className="memory-about-me" aria-label={`What ${name} remembers about you`}>
    <p className="ui-help">What {name} learned about how to work with you, in your own conversation with it. It uses these only when you ask it there. Only you can see them; remove any you don’t want it to keep.</p>
    {!data.notes.length && <p className="ui-empty">Nothing yet. When {name} learns how you like things done, it keeps a short note here.</p>}
    <ul className="memory-list">{data.notes.map(note => <li key={note.id} className="memory-note">
      <div className="memory-note-text"><p>{note.text}</p>
        <p className="memory-meta"><span className="ui-badge">{kindName(note.kind)}</span><span>{day(note.created_ms)}</span></p></div>
      <Button disabled={mutation.busy} aria-label={`Remove: ${note.text}`} onClick={() => void mutation.run(async () => {
        await forgetAboutMe(agent, note.id);
        mutation.setStatus('Removed.');
        await refresh();
      })}>Remove</Button>
    </li>)}</ul>
    <p className="ui-status" role="status">{mutation.busy ? 'Removing…' : mutation.status}</p>
  </section>;
}
