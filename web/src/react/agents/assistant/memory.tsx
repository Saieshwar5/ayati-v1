import { useCallback, useRef, useState, type FormEvent } from 'react';
import { Button } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { changeMemory, kinds, loadAssistantSettings, loadMemories, saveAssistantSettings, saveMemory, type Memory, type MemoryKind } from './settings-model';
import { MemoryLine } from './memory-line';

/** Your assistant's memory: what it remembers about you and your work, one
 * short line each. It adds lines as it works with you and when it looks back
 * at your chat; you add, change and remove any of them. Only you read this. */
export function AssistantMemory({ name }: { name: string }) {
  const load = useCallback(() => loadMemories(), []);
  const { data, error, refresh } = useResource(load);
  const [kind, setKind] = useState<MemoryKind>('about');
  const [text, setText] = useState('');
  // A retried save of the same words reuses its id, so it is kept once.
  const attempt = useRef<{ id: string; kind: MemoryKind; text: string } | undefined>(undefined);
  const mutation = useMutation();
  if (error && !data) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading what {name} remembers…</p>;
  if (data.available === false) return <p className="ui-help">Memory is not available in this workspace yet.</p>;
  const act = (action: () => Promise<unknown>, done: string, after?: () => void) => void mutation.run(async () => {
    await action();
    after?.();
    mutation.setStatus(done);
    await refresh();
  });
  const line = text.replace(/\s+/g, ' ').trim();
  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!line) return;
    const same = attempt.current?.kind === kind && attempt.current.text === line;
    const next = same ? attempt.current! : { id: crypto.randomUUID(), kind, text: line };
    attempt.current = next;
    act(() => saveMemory(next), 'Remembered.', () => { attempt.current = undefined; setText(''); });
  };
  const full = data.memories.length >= data.most;
  return <section className="assistant-memory" aria-label="Memory">
    <h3>Memory</h3>
    <p className="ui-help">What {name} remembers about you and your work, and uses in everything it does for you. It adds lines as it learns; change or remove any of them. Only you can read this.</p>
    {!data.memories.length && <p className="ui-help">Nothing yet. Add a line, or just work with {name}: it remembers what will matter later.</p>}
    {kinds.map(([id, title]) => {
      const lines = data.memories.filter(memory => memory.kind === id);
      return lines.length > 0 && <div key={id} className="assistant-memory-group">
        <h4>{title}</h4>
        <ul>{lines.map(memory => <MemoryLine key={memory.id} name={name} memory={memory} busy={mutation.busy}
          save={(next: Pick<Memory, 'kind' | 'text'>, saved: () => void) => act(() => saveMemory({ id: memory.id, ...next }), 'Saved.', saved)}
          remove={() => act(() => changeMemory(memory.id, 'remove'), 'Removed.')} />)}</ul>
      </div>;
    })}
    {!full && <form className="assistant-memory-add" onSubmit={add}>
      <select aria-label="What the line is about" value={kind} onChange={event => setKind(event.target.value as MemoryKind)}>
        {kinds.map(([id, title]) => <option key={id} value={id}>{title}</option>)}</select>
      <input aria-label="Something to remember" maxLength={300} value={text} onChange={event => setText(event.target.value)} placeholder="Add a line to remember" />
      <Button type="submit" primary disabled={mutation.busy || !line}>Remember</Button>
    </form>}
    {full && <p className="ui-help">That is the most lines {name} keeps. Remove some to add more.</p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {data.ended.length > 0 && <details className="assistant-memory-ended"><summary>{name} ended {data.ended.length === 1 ? '1 line' : `${data.ended.length} lines`}</summary>
      <p className="ui-help">{name} ended these because they stopped being true or you asked it to forget them. It no longer uses them.</p>
      <ul>{data.ended.map(memory => <li key={memory.id}><span>{memory.text}</span>
        <span className="assistant-memory-actions">
          <Button disabled={mutation.busy} aria-label={`Bring back: ${memory.text}`} onClick={() => act(() => changeMemory(memory.id, 'restore'), 'Brought back.')}>Bring back</Button>
          <Button disabled={mutation.busy} aria-label={`Delete: ${memory.text}`} onClick={() => act(() => changeMemory(memory.id, 'remove'), 'Deleted.')}>Delete</Button>
        </span></li>)}</ul>
    </details>}
    <LearningSwitch name={name} />
  </section>;
}

/** Whether the assistant keeps learning on its own; saved as soon as it is changed. */
function LearningSwitch({ name }: { name: string }) {
  const load = useCallback(() => loadAssistantSettings(), []);
  const { data, setData } = useResource(load);
  const mutation = useMutation();
  if (!data || data.available === false) return null;
  const on = data.learn ?? true;
  const change = (learn: boolean) => void mutation.run(async () => {
    await saveAssistantSettings({ learn });
    setData({ ...data, learn });
    mutation.setStatus(learn ? `${name} keeps learning.` : `${name} only remembers what you add or tell it.`);
  });
  return <>
    <label className="ui-check"><input type="checkbox" checked={on} disabled={mutation.busy} onChange={event => change(event.target.checked)} />
      <span>Keep learning on its own<small className="ui-help">{name} learns how you write from what you change in its drafts, and about once a day looks back at your chat with it to remember what will matter later. It tells you what it kept in that chat.</small></span></label>
    <p className="ui-status" role="status">{mutation.status}</p>
  </>;
}
