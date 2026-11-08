import { useRef, useState, type ReactNode } from 'react';
import { Button } from '../../ui';
import { EverythingItReceives } from '../system-prompt';
import { DraftEditor } from '../skills/draft-editor';
import { SetupChat, type SkillWriting } from '../skills/setup-chat';
import { applyAnswer, newDraft, type DraftSkill, type LibraryPick } from '../skills/writer-model';
import { starters, untouched } from './starters';

/** Create step 3 (designs/agent-built-skills 2): the person describes the job in
 * the setup chat and the writer drafts Ground rules (always on) and skills
 * (opened only when needed); nothing is saved until Create agent. Without a
 * writer both are written by hand. `children`: skills added from the library. */
export function SkillsStep({ name, title, text, change, apps, connections, writing, drafts, setDrafts, library, reuse, children }: {
  name: string; title: string; text: string; change: (text: string, title?: string) => void;
  apps: boolean; connections: string[]; writing: SkillWriting;
  drafts: DraftSkill[]; setDrafts: (update: (old: DraftSkill[]) => DraftSkill[]) => void;
  library: LibraryPick[]; reuse: (picks: LibraryPick[]) => void; children?: ReactNode;
}) {
  const [note, setNote] = useState('');
  const [previewing, setPreviewing] = useState(false);
  const [editing, setEditing] = useState<DraftSkill | 'new'>();
  const [focus, setFocus] = useState<string>();
  const input = useRef<HTMLTextAreaElement>(null);
  const display = name.trim() || 'this agent';
  const chat = writing.available;
  const start = (id: string) => {
    const starter = starters.find(item => item.id === id)!;
    if (chat) { writing.setComposer(starter.text(name)); input.current?.focus(); return; }
    if (!untouched(text, name)) { setNote('Starters fill an empty box. Clear the box first to use one.'); return; }
    setNote('');
    change(starter.text(name), title.trim() ? undefined : starter.title);
  };
  const submit = () => writing.send({ name: name.trim(), role: title, connections, ground_rules: text,
    skills: drafts.map(({ name, description, steps }) => ({ name, description, steps })), focus }, answer => {
    if (answer.ground_rules) change(answer.ground_rules);
    setDrafts(old => applyAnswer(old, answer));
    if (answer.reuse.length) reuse(answer.reuse);
    setFocus(undefined);
  });
  const ask = (draft: DraftSkill) => { setFocus(draft.name); writing.setComposer(`About ${draft.name}: `); input.current?.focus(); };
  const saveDraft = (fields: Pick<DraftSkill, 'name' | 'description' | 'steps'>) => {
    const changing = editing;
    setDrafts(old => changing === 'new' || !changing ? [...old, newDraft(fields)]
      : old.map(draft => draft.key === changing.key ? { ...draft, ...fields, id: crypto.randomUUID() } : draft));
    setEditing(undefined);
  };
  const starterRow = <div className="agent-starters" role="group" aria-label="Start from">
    <span>Start from</span>
    {starters.filter(starter => !chat || starter.id !== 'blank').map(starter => <Button key={starter.id} onClick={() => start(starter.id)}>{starter.label}</Button>)}
  </div>;
  const column = <div className="agent-skills-column">
    <label className="ui-field">Ground rules <small>Always on: who {display} is, how it talks, what it never does and when to ask a person</small>
      <textarea rows={chat ? 9 : 12} maxLength={24000} value={text} placeholder={`Who ${display} is, how it talks, what it must never do and when to ask a person.`}
        onChange={event => change(event.target.value)} />
    </label>
    <section className="agent-create-skills" aria-label="Skills">
      <h3>Skills <small>Steps for one kind of request, opened only when a request needs them</small></h3>
      {drafts.length > 0 && <ul className="draft-skills">{drafts.map(draft => <li key={draft.key}>
        <span className="draft-skill-text"><strong>{draft.name} <span className="draft-badge">New</span></strong>
          <span>{draft.description}</span><small>{draft.steps.split('\n').filter(line => line.trim()).length} steps</small></span>
        <span className="draft-actions">
          <Button onClick={() => setEditing(draft)} aria-label={`Edit ${draft.name}`}>Edit</Button>
          {chat && <Button onClick={() => ask(draft)} aria-label={`Ask ${display} about ${draft.name}`}>Ask {display}</Button>}
          <Button onClick={() => setDrafts(old => old.filter(item => item.key !== draft.key))} aria-label={`Remove ${draft.name}`}>Remove</Button>
        </span>
      </li>)}</ul>}
      {children}
      <Button onClick={() => setEditing('new')}>Write a skill</Button>
    </section>
  </div>;
  return <div className="agent-create-skills-step">
    <p className="agent-system-bar"><span><strong>Cuckoo system prompt</strong> · the same for every agent · nobody in the workspace can change it</span>
      <Button className="ui-link" onClick={() => setPreviewing(true)}>See everything {display} receives</Button></p>
    {chat
      ? <div className="agent-skills-writing">
        <SetupChat name={display} subtitle="Setting up · no tools; nothing is saved until you create it" writing={writing} submit={submit} input={input}
          greeting={`Hi, I’m ${display}. Tell me what I should do, how to talk to people and what I must never do. I’ll write my ground rules and skills for you to check.`}
          extra={writing.messages.length === 0 ? starterRow : undefined} />
        {column}
      </div>
      : <div className="agent-create-main">
        {!writing.loading && <p className="ui-help">Writing skills in chat isn’t set up here, so write the ground rules and skills yourself.</p>}
        {starterRow}
        {note && <p className="ui-help" role="status">{note}</p>}
        {column}
      </div>}
    {editing && <DraftEditor agent={display} draft={editing === 'new' ? undefined : editing} close={() => setEditing(undefined)} save={saveDraft}
      taken={drafts.filter(draft => editing === 'new' || draft.key !== editing.key).map(draft => draft.name)} />}
    {previewing && <EverythingItReceives draft={{ name: name.trim() || 'Your agent', role: title, instructions: text, apps,
      skills: [...drafts, ...library].map(({ name, description }) => ({ name, description })) }} close={() => setPreviewing(false)} />}
  </div>;
}
