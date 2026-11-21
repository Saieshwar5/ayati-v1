import { useState, type FormEvent } from 'react';
import './assistant.css';
import { Button } from '../../ui';

/** One rule per line; blank lines are not rules. */
export const rulesOf = (text: string) => text.split('\n').map(line => line.trim()).filter(Boolean);
const most = 40;

/** An assistant's rules: short lines it always follows, in place of an
 * instructions box. Adding or removing one saves at once; a failed save keeps
 * the list and what was typed. */
export function AssistantRules({ text, editable, busy, status, save }: {
  text: string; editable: boolean; busy: boolean; status: string;
  /** Saves the whole list; resolves to whether it was kept. */
  save: (text: string, done: string) => Promise<boolean>;
}) {
  const [adding, setAdding] = useState('');
  const rules = rulesOf(text);
  const rule = adding.replace(/\s+/g, ' ').trim();
  const add = (event: FormEvent) => {
    event.preventDefault();
    if (!rule || busy) return;
    void save([...rules, rule].join('\n'), 'Rule added.').then(kept => { if (kept) setAdding(''); });
  };
  return <section className="assistant-rules" aria-label="Rules">
    {!rules.length && <p className="ui-help">No rules yet.</p>}
    <ol className="assistant-rule-list">{rules.map((line, index) => <li key={`${index}:${line}`}>
      <span>{line}</span>
      {editable && <Button disabled={busy} aria-label={`Remove rule: ${line}`}
        onClick={() => void save(rules.filter((_, at) => at !== index).join('\n'), 'Rule removed.')}>Remove</Button>}
    </li>)}</ol>
    {editable && rules.length < most && <form className="assistant-rule-add" onSubmit={add}>
      <input aria-label="New rule" maxLength={300} value={adding} onChange={event => setAdding(event.target.value)} placeholder="Add a rule, one line" />
      <Button type="submit" primary disabled={busy || !rule}>Add rule</Button>
    </form>}
    {editable && rules.length >= most && <p className="ui-help">That is the most rules it keeps. Remove one to add another, or put longer steps in a skill.</p>}
    <p className="ui-status" role="status">{busy ? 'Saving…' : status}</p>
  </section>;
}
