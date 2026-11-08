import { useState, type ReactNode } from 'react';
import { Button } from '../../ui';
import { EverythingItReceives, SystemPromptCard } from '../system-prompt';
import { starters, untouched } from './starters';

/** Create step 2: one instructions box with starters, beside the locked Cuckoo
 * system prompt (design 2-create-instructions; designs/agent-built-skills 6).
 * `children` (the Skills row) sits under the box (designs/chat-and-account 7). */
export function InstructionsStep({ name, title, text, rules, apps, skills, change, children }: {
  name: string; title: string; text: string; rules: string; apps: boolean; skills: { name: string; description: string }[];
  change: (text: string, title?: string) => void; children?: ReactNode;
}) {
  const [note, setNote] = useState('');
  const [previewing, setPreviewing] = useState(false);
  const display = name.trim() || 'this agent';
  const start = (id: string) => {
    const starter = starters.find(item => item.id === id)!;
    if (!untouched(text, name)) { setNote('Starters fill an empty box. Clear the box first to use one.'); return; }
    setNote('');
    change(starter.text(name), title.trim() ? undefined : starter.title);
  };
  return <div className="agent-create-instructions">
    <div className="agent-create-main">
      <div className="agent-starters" role="group" aria-label="Start from">
        <span>Start from</span>
        {starters.map(starter => <Button key={starter.id} onClick={() => start(starter.id)}>{starter.label}</Button>)}
      </div>
      {note && <p className="ui-help" role="status">{note}</p>}
      <label className="ui-field">Instructions <small>Character, job and rules, in your own words</small>
        <textarea rows={14} maxLength={24000} value={text} placeholder={`What ${display} does, how it talks, what it must never do and when to ask a person.`}
          onChange={event => change(event.target.value)} />
      </label>
      {children}
    </div>
    <aside className="agent-create-side">
      <SystemPromptCard name={display} rules={rules} open={() => setPreviewing(true)} />
    </aside>
    {previewing && <EverythingItReceives draft={{ name: name.trim() || 'Your agent', role: title, instructions: text, apps, skills }} close={() => setPreviewing(false)} />}
  </div>;
}
