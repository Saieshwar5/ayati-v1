import type { Guard } from '../agents/control-model';

export function ApprovalFields({ fields }: { fields: unknown }) {
  if (!fields || typeof fields !== 'object' || Array.isArray(fields)) return null;
  return <dl className="control-fields">{Object.entries(fields).map(([field, value]) => <div key={field}>
    <dt>{field.replaceAll('_', ' ')}</dt><dd>{typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value)}</dd>
  </div>)}</dl>;
}

/** Why a company rule held or blocked this request, in the words the rule was saved with. */
export function GuardNote({ guard }: { guard: Guard }) {
  const { facts } = guard;
  const field = facts.field.replaceAll('_', ' ');
  const detail = !guard.known ? `The request didn't include a usable ${field}, so a person decides.`
    : facts.today_after !== undefined ? `Today so far: ${facts.today_before} · with this request: ${facts.today_after} · limit: ${String(facts.limit)}.`
    : `This request's ${field}: ${String(facts.value)}.`;
  return <div className={`approval-guard ${guard.effect}`} role="note">
    <strong>{guard.effect === 'block' ? 'Blocked by a company rule' : 'Held by a company rule'}</strong>
    <p>{guard.summary}</p><p className="ui-help">{detail}</p>
  </div>;
}
