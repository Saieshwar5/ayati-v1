import { useCallback, useState } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { loadAssistantSettings, saveAssistantSettings } from './settings-model';

type Notes = { about: string; style: string };

/** What your assistant knows about you and how you write: two short texts it is
 * given with every task. It adds lines itself; you change or remove anything. */
export function AboutYou({ name }: { name: string }) {
  const load = useCallback(() => loadAssistantSettings(), []);
  const { data, error } = useResource(load);
  const [saved, setSaved] = useState<Notes>();
  const [draft, setDraft] = useState<Notes>();
  const mutation = useMutation();
  if (error) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading what {name} knows about you…</p>;
  if (data.available === false) return <p className="ui-help">Notes about you are not available in this workspace yet.</p>;
  const base = saved ?? { about: data.about, style: data.style };
  const notes = draft ?? base;
  const changed = notes.about !== base.about || notes.style !== base.style;
  const edit = (change: Partial<Notes>) => { mutation.setStatus(''); setDraft({ ...notes, ...change }); };
  const save = () => void mutation.run(async () => {
    await saveAssistantSettings(notes);
    setSaved(notes); setDraft(undefined);
    mutation.setStatus('Saved.');
  });
  return <section className="assistant-about" aria-label={`What ${name} knows about you`}>
    <label className="ui-field">What {name} knows about you
      <textarea rows={6} maxLength={6000} value={notes.about} onChange={event => edit({ about: event.target.value })}
        placeholder="Your role, what matters to you this month, who you work with…" /></label>
    <p className="ui-help">{name} adds a line here when it learns something lasting about you. Change or remove anything. Only you can read this.</p>
    <label className="ui-field">How you write
      <textarea rows={4} maxLength={3000} value={notes.style} onChange={event => edit({ style: event.target.value })}
        placeholder="Short and friendly. No exclamation marks. Signs off with first name…" /></label>
    <p className="ui-help">Used when {name} drafts a reply or answers for you.</p>
    <div className="agent-save-row">
      <Button primary disabled={mutation.busy || !changed} onClick={save}>Save changes</Button>
      {changed && <Button disabled={mutation.busy} onClick={() => { setDraft(undefined); mutation.setStatus(''); }}>Discard</Button>}
      <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    </div>
  </section>;
}
