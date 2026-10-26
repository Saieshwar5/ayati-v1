import { useId, type ChangeEvent } from 'react';
import type { QuestionField } from './model';

export function FormFields({ fields, values, change }: { fields: QuestionField[]; values: Record<string, string>; change: (id: string, value: string) => void }) {
  const group = useId();
  return <>{fields.map(field => {
    const id = `${group}-${field.id}`, help = `${id}-help`, value = Object.hasOwn(values, field.id) ? values[field.id] : '';
    const props = { id, value, required: field.required, 'aria-describedby': help, onChange: (event: ChangeEvent<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>) => change(field.id, event.target.value) };
    return <div className="ui-field question-field" key={field.id}><label htmlFor={id}>{field.label}{field.required && <span aria-hidden="true"> *</span>}</label>
      {field.kind === 'multiline' ? <textarea {...props} maxLength={field.max_length ?? 4000} rows={3} />
        : field.kind === 'choice' ? <select {...props}><option value="">Choose…</option>{field.options.map(option => <option key={option.id} value={option.id}>{option.label}</option>)}</select>
          : field.kind === 'boolean' ? <select {...props}><option value="">Choose…</option><option value="true">Yes</option><option value="false">No</option></select>
            : <input {...props} type={field.kind === 'number' ? 'number' : field.kind === 'date' ? 'date' : 'text'}
              min={field.kind === 'number' ? field.min : field.kind === 'date' ? '0001-01-01' : undefined}
              max={field.kind === 'number' ? field.max : field.kind === 'date' ? '9999-12-31' : undefined}
              step={field.kind === 'number' ? 'any' : undefined} maxLength={field.max_length ?? 1000} />}
      <small id={help} className="ui-help">{field.description}{field.suggested !== undefined && `${field.description ? ' ' : ''}Suggested value shown; you can change it.`}</small>
    </div>;
  })}</>;
}
