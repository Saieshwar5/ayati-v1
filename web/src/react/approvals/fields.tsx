export function ApprovalFields({ fields }: { fields: unknown }) {
  if (!fields || typeof fields !== 'object' || Array.isArray(fields)) return null;
  return <dl className="control-fields">{Object.entries(fields).map(([field, value]) => <div key={field}>
    <dt>{field.replaceAll('_', ' ')}</dt><dd>{typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value)}</dd>
  </div>)}</dl>;
}
