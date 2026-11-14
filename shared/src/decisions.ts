/**
 * Exact approvals and agent questions, as people answer them on the web
 * (web/src/react/approvals/, web/src/react/questions/). Approving allows one
 * action exactly as shown; answering a question supplies facts and approves
 * nothing.
 */

export type Guard = { effect?: 'ask' | 'block'; summary: string };

export type Approval = {
  id: string;
  title: string;
  operation: string;
  effect: string;
  arguments: unknown;
  state: string;
  chat: string;
  thread: string | null;
  agent: string;
  agent_name: string;
  account_name: string;
  requester_name?: string;
  conversation_name?: string;
  can_decide: boolean;
  decision?: string | null;
  deciders?: { id: string; name: string }[];
  backup_at?: number;
  needs_other?: boolean;
  decided_by_name?: string | null;
  guard?: Guard | null;
};
export type Decision = 'approve' | 'reject' | 'request_changes';

/** What a chat message carries about its approval. */
export type ApprovalReference = {
  id?: string;
  state?: string;
  restricted?: boolean;
  may_decide?: boolean;
  waiting_for?: string[];
  backup_at?: number;
};

export type QuestionOption = { id: string; label: string };
export type QuestionField = {
  id: string;
  label: string;
  kind: string;
  required: boolean;
  description: string;
  options: QuestionOption[];
  suggested?: string | number | boolean;
  min?: number;
  max?: number;
  max_length?: number;
};
export type QuestionSpecification = {
  version: number;
  kind: string;
  title: string;
  description: string;
  fallback: string;
  options: QuestionOption[];
  fields: QuestionField[];
  allow_other: boolean;
  min_choices?: number;
  max_choices?: number;
};
export type Question = {
  id: string;
  key: string;
  revision: number;
  state: string;
  values: Record<string, unknown> | null;
  specification: QuestionSpecification;
  respondent: string;
  respondent_name: string;
  can_respond: boolean;
  reply: string;
  chat: string;
  agent?: string;
  answered_ms?: number;
};
export type QuestionReference = { id: string; title: string; state: string; respondent: string; respondent_name: string };
export type Answer = { id: string; revision: number; decision: 'answer' | 'decline'; values: Record<string, unknown> };

export const approvalStates: Record<string, string> = {
  pending: 'Waiting for approval',
  executing: 'Executing',
  completed: 'Completed',
  denied: 'Blocked',
  rejected: 'Rejected',
  invalidated: 'Permission changed',
  uncertain: 'Outcome unconfirmed',
  not_sent: 'Not sent',
};

/** "Ravi", "Ravi or Meera", "Ravi, Meera or Arjun". */
export function either(names: string[]): string {
  return names.length < 2 ? (names[0] ?? '') : `${names.slice(0, -1).join(', ')} or ${names[names.length - 1]}`;
}

/** An action's exact fields as [label, value] pairs, in the words people read. */
export function fieldEntries(fields: unknown): [string, string][] {
  if (!fields || typeof fields !== 'object' || Array.isArray(fields)) return [];
  return Object.entries(fields).map(([field, value]) => [
    field.replaceAll('_', ' '),
    typeof value === 'boolean' ? (value ? 'Yes' : 'No') : typeof value === 'object' && value !== null ? JSON.stringify(value) : String(value),
  ]);
}

/** The line under an approval card in chat, for the person looking at it. */
export function waitingLine(reference: ApprovalReference): string {
  if (reference.restricted) return 'Details are restricted to people with access to the source work.';
  if (reference.state !== 'pending') return approvalStates[reference.state ?? ''] ?? 'Saved request';
  if (reference.may_decide) return 'Waiting for your decision';
  return reference.waiting_for?.length ? `Waiting for approval from ${either(reference.waiting_for)}` : "Waiting for an admin's approval";
}

export const questionStates: Record<string, string> = {
  pending: 'Waiting for an answer',
  answered: 'Answer saved',
  declined: 'Question declined',
  invalidated: 'Question no longer active',
};

/** Kinds this client can answer; anything else shows the agent's fallback text. */
export function questionSupported(specification: QuestionSpecification): boolean {
  return specification.version === 1 && ['single_choice', 'multiple_choice', 'form'].includes(specification.kind);
}

export type QuestionDraft = { choices: string[]; other?: string; fields: Record<string, string> };

/** The values a draft answers with, typed as the web sends them. */
export function answerValues(specification: QuestionSpecification, draft: QuestionDraft): Record<string, unknown> {
  if (specification.kind !== 'form') return { choices: draft.choices, ...(draft.other !== undefined ? { other: draft.other } : {}) };
  return Object.fromEntries(
    specification.fields
      .filter(field => (draft.fields[field.id] ?? '') !== '')
      .map(field => {
        const value = draft.fields[field.id];
        return [field.id, field.kind === 'number' ? Number(value) : field.kind === 'boolean' ? value === 'true' : value];
      }),
  );
}

/** Why the draft can't be sent yet, or undefined when it can. */
export function draftProblem(specification: QuestionSpecification, draft: QuestionDraft): string | undefined {
  if (specification.kind === 'form') {
    const missing = specification.fields.find(field => field.required && !(draft.fields[field.id] ?? '').trim());
    return missing ? `Fill in ${missing.label}.` : undefined;
  }
  const count = draft.choices.length + (draft.other !== undefined ? 1 : 0);
  if (draft.other !== undefined && !draft.other.trim()) return 'Write your other answer.';
  const min = specification.kind === 'single_choice' ? 1 : (specification.min_choices ?? 1);
  const max = specification.kind === 'single_choice' ? 1 : (specification.max_choices ?? specification.options.length + Number(specification.allow_other));
  if (count < min) return min === 1 ? 'Choose an answer.' : `Choose at least ${min}.`;
  if (count > max) return `Choose at most ${max}.`;
  return undefined;
}

/** The saved answer in words, for an answered question. */
export function answerLabels(question: Question): { id: string; label: string; value: string }[] {
  const spec = question.specification;
  const values = question.values ?? {};
  if (spec.kind === 'form') {
    return spec.fields
      .filter(field => Object.hasOwn(values, field.id))
      .map(field => {
        const value = values[field.id];
        return {
          id: field.id,
          label: field.label,
          value:
            field.kind === 'boolean'
              ? value === true ? 'Yes' : 'No'
              : field.kind === 'choice'
                ? (field.options.find(option => option.id === value)?.label ?? String(value))
                : String(value),
        };
      });
  }
  const selected = Array.isArray(values.choices) ? values.choices : [];
  return [
    {
      id: 'choices',
      label: 'Selected',
      value: [
        ...selected.map(id => spec.options.find(option => option.id === id)?.label ?? String(id)),
        ...(typeof values.other === 'string' ? [values.other] : []),
      ].join(' · '),
    },
  ];
}
