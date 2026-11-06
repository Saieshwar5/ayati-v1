import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { replayRule, type Replay, type RuleInput } from './company-rules-model';

const brief = (fields: Record<string, unknown>) => Object.entries(fields).slice(0, 3).map(([k, v]) => `${k.replaceAll('_', ' ')} ${String(v)}`).join(' · ');

/** What a draft would have done to the last 30 days of requests, before saving it. */
export function ReplayPanel({ input }: { input?: RuleInput }) {
  const mutation = useMutation();
  const [result, setResult] = useState<{ key: string; replay: Replay }>();
  const key = JSON.stringify(input ?? null);
  const shown = result?.key === key ? result.replay : undefined;
  const run = () => input && void mutation.run(async () => setResult({ key, replay: await replayRule(input) }));
  return <section className="company-rule-replay" aria-label="Try it on past requests">
    <Button disabled={!input || mutation.busy} onClick={run}>{mutation.busy ? 'Trying…' : `Try it on the last 30 days`}</Button>
    {mutation.status && <p role="alert">{mutation.status}</p>}
    {shown && <div role="status">
      <p><strong>{shown.examined ? `Of ${shown.examined} matching requests, it would have held ${shown.would_hold} and blocked ${shown.would_block}.` : 'No matching requests in the last 30 days.'}</strong></p>
      {shown.examples.length > 0 && <ul>{shown.examples.map(example => <li key={example.id}>
        {example.effect === 'block' ? 'Block' : 'Ask'} · {example.title} · {example.day} · <small>{brief(example.arguments)}</small></li>)}</ul>}
    </div>}
  </section>;
}
