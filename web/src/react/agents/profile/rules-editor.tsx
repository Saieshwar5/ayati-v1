import { useEffect, useRef, useState } from 'react';
import { Button } from '../../ui';
import { useMutation } from '../../requests';
import { saveAgent, type Agent } from '../model';

/**
 * The agent's ground rules, edited where they are read. `start` is the text the
 * box opens with: the saved rules, a starter, or what the assistant proposed.
 * The box keeps the revision it opened at, so a save never overwrites a change
 * someone else made meanwhile: the server refuses it, and the box and what was
 * typed stay. Escape and Cancel put the saved text back.
 */
export function RulesEditor({ agent, start, close, saved }: { agent: Agent; start: string; close: () => void; saved: () => Promise<void> }) {
  const [text, setText] = useState(start);
  const [base] = useState(agent);
  const box = useRef<HTMLTextAreaElement>(null);
  const mutation = useMutation();
  useEffect(() => { const node = box.current; if (node) { node.focus(); node.setSelectionRange(node.value.length, node.value.length); } }, []);
  // A newer proposal while the box is open replaces what it holds.
  const [shown, setShown] = useState(start);
  if (shown !== start) { setShown(start); setText(start); }
  const save = () => void mutation.run(async () => {
    await saveAgent({ ...base, people: undefined, settings: { ...base.settings, instructions: text.trim() } });
    await saved();
    close();
  });
  return <form className="profile-rules-edit" aria-label="Rules" onSubmit={event => { event.preventDefault(); save(); }}
    onKeyDown={event => { if (event.key === 'Escape' && !mutation.busy) { event.stopPropagation(); close(); } }}>
    <textarea ref={box} aria-label={`Rules for ${agent.name}`} rows={Math.min(16, Math.max(5, text.split('\n').length + 1))} maxLength={24000}
      value={text} disabled={mutation.busy} onChange={event => setText(event.target.value)} />
    <div className="profile-save">
      <Button primary type="submit" disabled={mutation.busy || text.trim() === base.settings.instructions.trim()}>{mutation.busy ? 'Saving…' : 'Save'}</Button>
      <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
      {mutation.status && <span role="alert">{mutation.status}</span>}
    </div>
  </form>;
}
