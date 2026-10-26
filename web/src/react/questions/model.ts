export type QuestionOption = { id: string; label: string };
export type QuestionField = { id: string; label: string; kind: string; required: boolean; description: string;
  options: QuestionOption[]; suggested?: string | number | boolean; min?: number; max?: number; max_length?: number };
export type QuestionSpecification = { version: number; kind: string; title: string; description: string; fallback: string;
  options: QuestionOption[]; fields: QuestionField[]; allow_other: boolean; min_choices?: number; max_choices?: number };
export type QuestionReference = { id: string; title: string; state: string; respondent: string; respondent_name: string };
export type Question = { id: string; key: string; revision: number; state: string; values: Record<string, unknown> | null;
  specification: QuestionSpecification; respondent: string; respondent_name: string; can_respond: boolean;
  reply: string; chat: string; answered_by?: string; answered_ms?: number };
export const questionState = (state: string) => ({ pending: 'Waiting for an answer', answered: 'Answer saved', declined: 'Question declined', invalidated: 'Question no longer active' }[state] ?? 'Unavailable');
export function answerLabels(question: Question): { id: string; label: string; value: string }[] {
  const spec = question.specification, values = question.values ?? {};
  if (spec.kind === 'form') return spec.fields.filter(field => Object.hasOwn(values, field.id)).map(field => {
    const value = values[field.id];
    return { id: field.id, label: field.label, value: field.kind === 'boolean' ? value === true ? 'Yes' : 'No' : field.kind === 'choice' ? field.options.find(option => option.id === value)?.label ?? String(value) : String(value) };
  });
  const selected = Array.isArray(values.choices) ? values.choices : [];
  return [{ id: 'choices', label: 'Selected', value: [...selected.map(id => spec.options.find(option => option.id === id)?.label ?? String(id)), ...(typeof values.other === 'string' ? [values.other] : [])].join(' · ') }];
}
