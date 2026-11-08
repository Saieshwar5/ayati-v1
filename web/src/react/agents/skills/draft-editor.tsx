import { useState } from 'react';
import { Button, Dialog } from '../../ui';
import { slug, type DraftSkill } from './writer-model';

type Fields = Pick<DraftSkill, 'name' | 'description' | 'steps'>;

/** Write a skill, or change a draft, by hand: a name, "Use it when" and steps.
 * Nothing is saved until the agent is created. `taken`: other drafts' names. */
export function DraftEditor({ agent, draft, taken, close, save }: {
  agent: string; draft?: Fields; taken: string[]; close: () => void; save: (fields: Fields) => void;
}) {
  const [name, setName] = useState(draft?.name ?? '');
  const [description, setDescription] = useState(draft?.description ?? '');
  const [steps, setSteps] = useState(draft?.steps ?? '');
  const [problem, setProblem] = useState('');
  const saved = slug(name);
  const submit = () => {
    if (!saved) { setProblem('Give the skill a name with letters or numbers.'); return; }
    if (taken.includes(saved)) { setProblem(`${agent} already has a skill called ${saved}.`); return; }
    if (!description.trim() || !steps.trim()) { setProblem('Write when to use it and its steps.'); return; }
    save({ name: saved, description: description.replace(/\s+/g, ' ').trim(), steps: steps.trim() });
  };
  return <Dialog drawer title={draft ? `Change ${draft.name}` : 'Write a skill'} close={close}
    footer={<><Button onClick={close}>Cancel</Button><Button primary onClick={submit}>Save draft</Button></>}>
    <div className="draft-editor">
      <p className="ui-help">Steps {agent} follows for one kind of request, opened only when needed.</p>
      <label className="ui-field">Name
        <input value={name} maxLength={80} onChange={event => { setName(event.target.value); setProblem(''); }} /></label>
      {saved && <p className="ui-help">Saved as {saved}. Agents find a skill by this name.</p>}
      <label className="ui-field">Use it when <small>{agent} reads this line with every message to decide whether to open the skill.</small>
        <textarea rows={2} maxLength={1024} value={description} placeholder="Use when a customer asks for a refund"
          onChange={event => { setDescription(event.target.value); setProblem(''); }} /></label>
      <label className="ui-field">Steps <small>Plain words, one step per line.</small>
        <textarea rows={10} value={steps} placeholder={'1. Find the order.\n2. …'}
          onChange={event => { setSteps(event.target.value); setProblem(''); }} /></label>
      {problem && <p role="alert">{problem}</p>}
    </div>
  </Dialog>;
}
