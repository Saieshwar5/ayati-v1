import { useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation } from '../../requests';
import { localMinute, matchingInstants, type BusinessClock } from '../../../business-time';
import { storageScope, workspaceSession } from '../../../workspace-context';
import { cadence, date, results, saveSchedule, type Directory, type ScheduleInput } from './model';
function pending(key: string): ScheduleInput | undefined {
  try {
    const value = JSON.parse(window.sessionStorage.getItem(key) ?? 'null');
    if (value && typeof value.id === 'string' && (value.chat === null || typeof value.chat === 'string') && typeof value.name === 'string' && typeof value.content === 'string' && typeof value.timezone === 'string' && Number.isSafeInteger(value.first_ms) && (value.interval_seconds === null || Number.isSafeInteger(value.interval_seconds))) return value;
  } catch { /* Optional exact-save receipt; no provider credentials are stored. */ }
}
/** Common repeats are one choice; anything else is a custom interval (elapsed time). */
const presets = [['once', 'Once', null], ['day', 'Every day', 86400], ['week', 'Every week', 604800], ['interval', 'Custom…', undefined]] as const;
type Repeat = typeof presets[number][0];
const repeatOf = (seconds: number | null | undefined): Repeat => !seconds ? 'once' : seconds === 86400 ? 'day' : seconds === 604800 ? 'week' : 'interval';
/** "Every week, starting Fri 14 Nov 2026, 5:00 pm" — the schedule as one sentence. */
export function whenSentence(input: Pick<ScheduleInput, 'first_ms' | 'interval_seconds' | 'timezone'>): string {
  const first = date(input.first_ms, input.timezone);
  if (input.interval_seconds === null) return `Once, on ${first}`;
  const label = presets.find(([, , seconds]) => seconds === input.interval_seconds)?.[1] ?? cadence(input.interval_seconds);
  return `${label}, starting ${first}`;
}
export function CreateSchedule({ agent, agentName = 'The agent', data, clock, close, saved }: { agent: string; agentName?: string; data: Directory; clock: BusinessClock; close: () => void; saved: () => Promise<void> }) {
  const key = storageScope(`relay.schedule.pending.${agent}`);
  const [original] = useState(() => pending(key));
  const [name, setName] = useState(original?.name ?? '');
  const [content, setContent] = useState(original?.content ?? '');
  // Results go to Activity; posting a short summary to a conversation is optional.
  const [post, setPost] = useState(Boolean(original?.chat));
  const [chat, setChat] = useState(original?.chat ?? data.conversations[0]?.id ?? '');
  const [start, setStart] = useState(() => localMinute(original?.first_ms ?? Date.now() + 3600000, original?.timezone ?? clock.timezone));
  const [repeat, setRepeat] = useState<Repeat>(repeatOf(original?.interval_seconds));
  const [amount, setAmount] = useState(original?.interval_seconds ? String(original.interval_seconds / 60) : '1');
  const [unit, setUnit] = useState('60');
  const [review, setReview] = useState<ScheduleInput | undefined>(original);
  const attempt = useRef<ScheduleInput | undefined>(original);
  const mutation = useMutation();
  const alive = useRef(false);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const [confirmed, setConfirmed] = useState(false);
  const zone = original?.timezone ?? clock.timezone;
  const target = review ? review.chat : post ? chat : null;
  const destination = target ? data.conversations.find(item => item.id === target) : undefined;
  const ready = target === null || Boolean(destination);
  const submit = () => void mutation.run(async () => {
    if (!review || !data.execution_available || !ready) throw new Error('Refresh current conversation access before saving.');
    const exact = attempt.current ?? review;
    attempt.current = exact;
    try { window.sessionStorage.setItem(key, JSON.stringify(exact)); } catch { /* Optional; exact payload remains in memory. */ }
    await saveSchedule(agent, exact);
    try { window.sessionStorage.removeItem(key); } catch { /* Optional. */ }
    if (!alive.current) return;
    setConfirmed(true);
    try { await saved(); } catch { throw new Error('Schedule saved, but its list could not refresh. Close this review and refresh schedules.'); }
  });
  const inspect = () => void mutation.run(async () => {
    const times = matchingInstants(start, zone);
    if (times.length !== 1 || times[0] <= Date.now()) throw new Error('Choose a future local time that is not missing or repeated by daylight saving.');
    const interval = repeat === 'interval' ? Number(amount) * Number(unit) : presets.find(([value]) => value === repeat)![2] ?? null;
    if (interval !== null && (!Number.isSafeInteger(interval) || interval < 1 || interval > 31536000)) throw new Error('Choose an interval of at most 365 24-hour periods.');
    if (!name.trim() || new TextEncoder().encode(name).length > 100 || !content.trim() || new TextEncoder().encode(content).length > 10000) throw new Error('Give it a name (up to 100 bytes) and say what to do (up to 10,000 bytes).');
    if (!ready) throw new Error('Choose a conversation you may post to, or keep results in Activity only.');
    setReview({ id: crypto.randomUUID(), chat: target, name, content, first_ms: times[0], interval_seconds: interval, timezone: zone });
  });
  const footer = confirmed ? <Button onClick={close}>Done</Button> : review ? <>
    {!attempt.current && <Button disabled={mutation.busy} onClick={() => setReview(undefined)}>Back to timing</Button>}
    <Button primary disabled={mutation.busy || !ready || !data.execution_available} onClick={submit}>{attempt.current ? 'Retry same save' : 'Save schedule'}</Button>
  </> : <Button primary disabled={mutation.busy || !data.execution_available || !ready} onClick={inspect}>Review schedule</Button>;
  return <Dialog drawer title={review ? 'Check and save' : 'New schedule'} close={close} busy={mutation.busy} status={mutation.status} footer={footer}>
    {confirmed ? <p role="status">Schedule saved. Its runs and anything that needs you appear in Activity.</p> : review ? <div className="routine-review">
      {attempt.current && <p role="status">A save was not acknowledged. Retry this exact request safely; it cannot create a second routine. Inspect saved schedules before starting a different one.</p>}
      {attempt.current && mutation.status && <div><p className="ui-help">Editing as a new schedule leaves any previously saved routine unchanged. Check saved schedules first to avoid creating another routine for the same work.</p><Button onClick={() => {
        attempt.current = undefined; try { window.sessionStorage.removeItem(key); } catch { /* Optional local retry receipt. */ }
        setReview(undefined); mutation.setStatus('');
      }}>Edit as a new schedule</Button></div>}
      <h3>{review.name}</h3>
      <div className="routine-sentence"><p><strong>{whenSentence(review)}</strong> ({review.timezone}), {agentName} will:</p>
        <p className="routine-instructions">{review.content}</p></div>
      <dl className="agent-facts"><dt>Results</dt><dd>{results(review.chat, destination?.name ?? null)}</dd><dt>Set up by</dt><dd>{workspaceSession?.user?.display_name ?? workspaceSession?.user?.email ?? 'You'}</dd></dl>
      <p>Each run happens in the schedule's own room and appears in Activity, using your current access to the agent{review.chat ? ' and to the conversation it posts to' : ''}. Normal questions and exact tool approvals still apply. This grants no new permissions.</p>
      <p className="ui-help">Runs can use your configured model while you are offline. The service and background workers must be running. Reading this review starts no work.</p>
    </div> : <div className="ui-form">
      <label className="ui-field">Name<input value={name} maxLength={100} placeholder="Weekly delayed orders" onChange={event => setName(event.target.value)} /></label>
      <label className="ui-field">What should {agentName} do?<textarea rows={5} maxLength={10000} value={content} placeholder="List orders more than 3 days late and who is waiting on them." onChange={event => setContent(event.target.value)} /></label>
      <fieldset className="routine-results"><legend>Where results go</legend>
        <label className="routine-choice"><input type="radio" name="results" checked={!post} onChange={() => setPost(false)} />
          <span><strong>Activity only</strong><small>Results, approvals and questions appear in Activity. No conversation gets a message.</small></span></label>
        <label className="routine-choice"><input type="radio" name="results" checked={post} disabled={!data.conversations.length} onChange={() => setPost(true)} />
          <span><strong>Activity, and post the result to</strong><small>One short message with a link to the run. The steps, approvals and questions stay in Activity.</small></span></label>
        {post && <label className="ui-field">Conversation<select aria-label="Conversation" value={chat} onChange={event => setChat(event.target.value)}>{data.conversations.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>}
        {!data.conversations.length && <p className="ui-help">To post results, first add this agent to a conversation you may use.</p>}
        {post && data.more_conversations && <p className="ui-help">Only the first 100 eligible conversations are listed.</p>}
      </fieldset>
      <label className="ui-field">First run<input type="datetime-local" value={start} onChange={event => setStart(event.target.value)} /></label>
      <p className="ui-help">{clock.location} · {zone}. Repeated or missing daylight-saving times are rejected.</p>
      <fieldset className="routine-repeat"><legend>Repeat</legend>
        {presets.map(([value, label]) => <label key={value} className="routine-repeat-choice"><input type="radio" name="repeat" value={value} checked={repeat === value} onChange={() => setRepeat(value)} />{label}</label>)}
      </fieldset>
      {repeat === 'interval' && <fieldset><legend>Custom interval</legend><div className="routine-interval">
        <label className="ui-field">Every<input type="number" min="1" step="1" value={amount} onChange={event => setAmount(event.target.value)} /></label>
        <label className="ui-field">Unit<select aria-label="Unit" value={unit} onChange={event => setUnit(event.target.value)}><option value="60">minutes</option><option value="3600">hours</option><option value="86400">24-hour periods</option></select></label>
      </div><p className="ui-help">Elapsed time, not calendar recurrence. Local clock time can shift with daylight saving.</p></fieldset>}
      <p className="ui-help">Instructions and timing are saved together. To change them later, pause this schedule and make another.</p>
    </div>}
  </Dialog>;
}
