import { useId, useState } from 'react';
import { Button } from '../ui';
import { FormFields } from './form-fields';
import type { Question } from './model';

export function QuestionForm({ question, busy, respond }: { question: Question; busy: boolean; respond: (decision: 'answer' | 'decline', values: Record<string, unknown>) => void }) {
  const spec = question.specification, group = useId();
  const [fields, setFields] = useState<Record<string, string>>(() => Object.fromEntries(spec.fields.filter(field => field.suggested !== undefined).map(field => [field.id, String(field.suggested)])));
  const [choices, setChoices] = useState<string[]>([]), [alternative, setAlternative] = useState(false), [other, setOther] = useState('');
  const multiple = spec.kind === 'multiple_choice';
  const submit = () => {
    if (spec.kind !== 'form') { respond('answer', { choices, ...(alternative ? { other } : {}) }); return; }
    const values = Object.fromEntries(spec.fields.filter(field => Object.hasOwn(fields, field.id) && fields[field.id] !== '').map(field => {
      const value = fields[field.id];
      return [field.id, field.kind === 'number' ? Number(value) : field.kind === 'boolean' ? value === 'true' : value];
    }));
    respond('answer', values);
  };
  return <form className="ui-form question-form" aria-label="Answer question" onSubmit={event => { event.preventDefault(); submit(); }}>
    <fieldset disabled={busy}><legend className="question-form-legend">{spec.kind === 'form' ? 'Your answer' : multiple ? 'Choose options' : 'Choose one option'}</legend>
      {spec.kind === 'form' ? <FormFields fields={spec.fields} values={fields} change={(id, value) => setFields(previous => ({ ...previous, [id]: value }))} />
        : <><p className="ui-help">{multiple ? `Select ${spec.min_choices ?? 1}–${spec.max_choices ?? spec.options.length + Number(spec.allow_other)} answers, then submit.` : 'Select an option, then submit.'}</p>
          <div className="question-options">{spec.options.map(option => <label className="question-option" key={option.id}>
            <input type={multiple ? 'checkbox' : 'radio'} name={group} checked={choices.includes(option.id)} onChange={event => {
              if (!multiple) { setChoices([option.id]); setAlternative(false); }
              else { const checked = event.target.checked; setChoices(previous => checked ? [...previous, option.id] : previous.filter(id => id !== option.id)); }
            }} /><span>{option.label}</span>
          </label>)}{spec.allow_other && <label className="question-option"><input type={multiple ? 'checkbox' : 'radio'} name={group} checked={alternative} onChange={event => { setAlternative(event.target.checked); if (!multiple) setChoices([]); }} /><span>Another answer</span></label>}</div>
          {alternative && <label className="ui-field">Another answer<input type="text" value={other} required maxLength={1000} onChange={event => setOther(event.target.value)} /></label>}</>}
      <div className="question-form-actions"><Button primary type="submit">{busy ? 'Saving answer…' : 'Submit answer'}</Button><Button onClick={() => respond('decline', {})}>Decline question</Button></div>
    </fieldset>
    <p className="ui-help">Declining supplies no answer. Stopping the whole task is a separate control. Keep passwords and account credentials in account setup.</p>
  </form>;
}
