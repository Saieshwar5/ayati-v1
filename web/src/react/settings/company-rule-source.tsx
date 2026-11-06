import type { RuleAction, RuleSource } from './company-rules-model';

/** Fields a reading action shares with the action it guards, to match the same record. */
export function sharedFields(action: RuleAction | undefined, read: RuleAction | undefined): string[] {
  if (!action || !read) return [];
  return action.fields.filter(f => read.fields.some(r => r.name === f.name)).map(f => f.name);
}

/** Where a rule's value comes from: this request, or an earlier read in the same task
 * (core-agent CA1 part 3). For "Read first" the read is required and has no value. */
export function ReadSource({ reads, action, source, change, required }: {
  reads: RuleAction[]; action?: RuleAction; source: RuleSource | null; change: (next: RuleSource | null) => void; required: boolean;
}) {
  const read = reads.find(r => r.operation === source?.operation);
  const shared = sharedFields(action, read);
  const pick = (operation: string) => {
    if (!operation) { change(null); return; }
    const next = reads.find(r => r.operation === operation);
    change({ operation, match: sharedFields(action, next)[0] ?? '', path: required ? undefined : source?.path ?? '' });
  };
  return <>
    <label>{required ? 'Reading action' : 'Taken from'}<select value={source?.operation ?? ''} onChange={event => pick(event.target.value)}>
      {!required && <option value="">This request</option>}
      {required && !source && <option value="">Choose a reading action</option>}
      {reads.map(r => <option key={r.operation} value={r.operation}>{r.title}{required ? '' : ', read earlier in the task'}</option>)}
    </select></label>
    {source && <label>Matched on<select value={source.match} onChange={event => change({ ...source, match: event.target.value })}>
      {shared.map(name => <option key={name} value={name}>{name.replaceAll('_', ' ')}</option>)}</select></label>}
    {source && !required && <label>Field in the result<input value={source.path ?? ''} placeholder="total" onChange={event => change({ ...source, path: event.target.value.trim() })} /></label>}
  </>;
}
