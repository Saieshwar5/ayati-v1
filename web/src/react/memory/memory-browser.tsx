import { useCallback, useState, type FormEvent } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { changeNote, day, loadMemory, saveNote, saveTopic, undoChange, type Change, type Missed, type Note, type NoteChange, type Topic, type View } from './memory-model';
import { AddNote, NoteRow, TopicRow, WaitingNote } from './memory-notes';
import './memory.css';

const views: [View, string][] = [
  ['topics', 'Topics'], ['notes', 'Notes'], ['essentials', 'Essentials'], ['fading', 'Fading'],
  ['archive', 'Archive'], ['changes', 'Changes'], ['missed', 'Looked for, not found'],
];
const counted: Partial<Record<View, 'topics' | 'notes' | 'essentials' | 'fading' | 'archive' | 'waiting'>> = {
  topics: 'topics', notes: 'notes', essentials: 'essentials', fading: 'fading', archive: 'archive', waiting: 'waiting',
};
const empty: Record<View, string> = {
  topics: 'No topics yet. Notes with a topic, such as a customer or a system, are grouped here.',
  notes: 'Nothing remembered yet. Agents add notes as they learn while working.',
  essentials: 'No essentials yet. Pinned notes and the strongest ones are given to agents with every request.',
  fading: 'Nothing is fading. Notes nobody uses for a while are listed here before they go to the archive.',
  archive: 'The archive is empty.',
  waiting: 'Nothing waits for an answer.',
  changes: 'The nightly tidy-up has changed nothing yet.',
  missed: 'Agents found what they looked for in the last 30 days.',
};

/**
 * One memory page: the workspace's (Settings › Memory, admins) or one agent's
 * job and room notes (its page, owners and admins). Topics first, then every
 * note, the essentials, what is fading, the archive, what the tidy-up changed
 * and what agents looked for and did not find. Notes waiting for a person sit on top.
 */
export function MemoryBrowser({ agent }: { agent?: string }) {
  const [view, setView] = useState<View>('topics');
  const [search, setSearch] = useState('');
  const [asked, setAsked] = useState('');
  const [topic, setTopic] = useState('');
  const [adding, setAdding] = useState(false);
  const load = useCallback(() => loadMemory(agent, view, asked, topic), [agent, view, asked, topic]);
  const { data, error, refresh } = useResource(load);
  const waiting = useResource(useCallback(() => loadMemory(agent, 'waiting'), [agent]));
  const mutation = useMutation();
  const act = (action: () => Promise<unknown>, done: string, after?: () => void) => void mutation.run(async () => {
    await action();
    after?.();
    mutation.setStatus(done);
    await Promise.all([refresh(), waiting.refresh()]);
  });
  const change = (note: Note) => (next: NoteChange, done: string) => act(() => changeNote(agent, note.id, next), done);
  const show = (next: View, nextTopic = '') => { setView(next); setTopic(nextTopic); };
  if (error && !data) return <p role="alert">{error}</p>;
  if (data && !data.available) return <p className="ui-help">Memory is not available in this workspace yet.</p>;
  const counts = data?.counts;
  const waitingNotes = (waiting.data?.items ?? []) as Note[];
  const items = data?.items ?? [];
  // Rows are drawn by the view they were read for, never the one still loading.
  const shown = data?.view ?? view;
  return <div className="memory-browser">
    {counts && <p className="memory-facts">
      <span><b>{counts.notes.toLocaleString()}</b> notes</span><span><b>{counts.topics.toLocaleString()}</b> topics</span>
      <span><b>{counts.essentials}</b> essentials</span><span>used <b>{(data?.used_month ?? 0).toLocaleString()}</b> times in 30 days</span>
    </p>}
    {waitingNotes.length > 0 && view !== 'waiting' && <section className="settings-waiting-panel memory-needs" aria-label="Needs your answer">
      <h3>Needs your answer · {waitingNotes.length}</h3>
      {waitingNotes.slice(0, 2).map(note => <WaitingNote key={note.id} note={note} busy={mutation.busy} change={change(note)} />)}
      {waitingNotes.length > 2 && <Button onClick={() => show('waiting')}>See all {waitingNotes.length}</Button>}
    </section>}
    <div className="settings-filters" role="group" aria-label="Show">
      {views.map(([id, label]) => <Button key={id} aria-pressed={view === id} primary={view === id} onClick={() => show(id)}>
        {label}{counted[id] && counts ? <small className="memory-count">{counts[counted[id]!]}</small> : null}</Button>)}
      {view === 'waiting' && <Button aria-pressed primary>Waiting<small className="memory-count">{counts?.waiting}</small></Button>}
    </div>
    {['topics', 'notes', 'essentials', 'fading', 'archive'].includes(view) && <div className="memory-tools">
      <form role="search" className="memory-search" onSubmit={(event: FormEvent) => { event.preventDefault(); setAsked(search.trim()); }}>
        <input type="search" aria-label="Search the memory" placeholder="Search the memory" value={search} onChange={event => setSearch(event.target.value)} />
        <Button type="submit">Search</Button>
      </form>
      {!adding && <Button onClick={() => setAdding(true)}>Add a note</Button>}
    </div>}
    {adding && <AddNote busy={mutation.busy} close={() => setAdding(false)}
      add={(note, done) => act(() => saveNote(agent, note), 'Saved.', () => { done(); setAdding(false); })} />}
    {topic && <p className="memory-topic-filter">Notes about <strong>{topic}</strong> <Button onClick={() => show('topics')}>All topics</Button></p>}
    <p className="ui-status" role="status">{mutation.busy ? 'Saving…' : mutation.status}</p>
    {!data && <p role="status">Loading the memory…</p>}
    {data && !items.length && <p className="ui-empty">{asked ? `Nothing matches “${asked}”.` : empty[shown]}</p>}
    {data && shown === 'topics' && <ul className="memory-list">{(items as Topic[]).map(item => <TopicRow key={item.topic} topic={item} busy={mutation.busy}
      open={() => show('notes', item.topic)} save={(summary, done) => act(() => saveTopic(agent, item.topic, summary), summary ? 'Summary saved.' : 'Summary removed.', done)} />)}</ul>}
    {data && ['notes', 'essentials', 'fading', 'archive'].includes(shown) && <ul className="memory-list">
      {(items as Note[]).map(note => <NoteRow key={note.id} note={note} busy={mutation.busy} change={change(note)} />)}</ul>}
    {data && shown === 'waiting' && (items as Note[]).map(note => <WaitingNote key={note.id} note={note} busy={mutation.busy} change={change(note)} />)}
    {data && shown === 'changes' && <ul className="memory-list">{(items as Change[]).map(item => <li key={item.id} className="memory-change">
      <p>{item.summary}<small>{day(item.at_ms)}{item.undone_ms ? ' · undone' : ''}</small></p>
      {!item.undone_ms && <Button disabled={mutation.busy} aria-label={`Undo: ${item.summary}`} onClick={() => act(() => undoChange(agent, item.id), 'Undone.')}>Undo</Button>}
    </li>)}</ul>}
    {data && shown === 'missed' && <>
      <p className="ui-help">What agents searched their memory for in the last 30 days and did not find, from conversations you can open. Add a note to teach them.</p>
      <ul className="memory-list">{(items as Missed[]).map(item => <li key={item.query} className="memory-change">
        <p>“{item.query}”<small>{item.agents.join(', ')} · {item.times === 1 ? 'once' : `${item.times} times`} · last {day(item.last_ms)}</small></p>
      </li>)}</ul></>}
  </div>;
}
