import { useState, type ReactNode } from 'react';
import { Button } from '../../ui';
import { EverythingItReceives } from '../system-prompt';
import { starters, untouched } from './starters';

/** Create step 3: Ground rules, always on, with starters, and the skills opened
 * only when needed (`children`), under the locked Cuckoo system prompt
 * (designs/agent-built-skills 2). */
export function SkillsStep({ name, title, text, apps, skills, change, children }: {
  name: string; title: string; text: string; apps: boolean; skills: { name: string; description: string }[];
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
  return <div className="agent-create-skills-step">
    <p className="agent-system-bar"><span><strong>Cuckoo system prompt</strong> · the same for every agent · nobody in the workspace can change it</span>
      <Button className="ui-link" onClick={() => setPreviewing(true)}>See everything {display} receives</Button></p>
    <div className="agent-create-main">
      <div className="agent-starters" role="group" aria-label="Start from">
        <span>Start from</span>
        {starters.map(starter => <Button key={starter.id} onClick={() => start(starter.id)}>{starter.label}</Button>)}
      </div>
      {note && <p className="ui-help" role="status">{note}</p>}
      <label className="ui-field">Ground rules <small>Always on: who {display} is, how it talks, what it never does and when to ask a person</small>
        <textarea rows={12} maxLength={24000} value={text} placeholder={`Who ${display} is, how it talks, what it must never do and when to ask a person.`}
          onChange={event => change(event.target.value)} />
      </label>
      {children}
    </div>
    {previewing && <EverythingItReceives draft={{ name: name.trim() || 'Your agent', role: title, instructions: text, apps, skills }} close={() => setPreviewing(false)} />}
  </div>;
}
