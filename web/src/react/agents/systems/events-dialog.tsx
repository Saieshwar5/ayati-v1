import { useCallback, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { forgetSubject, loadSystemEvents, sendTestEvent, switchEvents, validType, type System } from './model';

/** A system's events (C2): turn them on with a key shown once, see what
 * arrived (field names only, never values) and send a test event. */
export function EventsDialog({ agent, system, canChange, close, changed }: {
  agent: string; system: System; canChange: boolean; close: () => void; changed: () => Promise<void>;
}) {
  const load = useCallback(() => loadSystemEvents(agent, system.id), [agent, system.id]);
  const { data, error, refresh } = useResource(load);
  const [key, setKey] = useState<string | null>(null);
  const [confirmOff, setConfirmOff] = useState(false);
  const [type, setType] = useState('');
  const [sample, setSample] = useState('{}');
  const [forget, setForget] = useState('');
  const m = useMutation();
  const turn = (on: boolean) => void m.run(async () => {
    const answer = await switchEvents(agent, system.id, on);
    setKey(answer.key); setConfirmOff(false);
    await refresh(); await changed();
    m.setStatus(on ? 'Events are on. Copy the key now; it is not shown again.' : 'Events are off. The system can no longer start work.');
  });
  const test = () => void m.run(async () => {
    if (!validType(type)) throw new Error('Use an event type such as exam.result_published.');
    let parsed: unknown;
    try { parsed = JSON.parse(sample || '{}'); } catch { throw new Error('The sample data must be JSON, such as {"exam":"Maths"}.'); }
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) throw new Error('The sample data must be a JSON object.');
    const answer = await sendTestEvent(agent, system.id, type, parsed as Record<string, unknown>);
    await refresh();
    m.setStatus(answer.matched ? `Test event sent; ${answer.matched} ${answer.matched === 1 ? 'automation runs' : 'automations run'}. See Activity.` : 'Test event sent. No automation listens to this type yet.');
  });
  const forgetOne = () => void m.run(async () => {
    if (!forget.trim()) throw new Error('Name the subject to forget, such as stu-104.');
    const answer = await forgetSubject(agent, system.id, forget.trim());
    setForget('');
    m.setStatus(answer.forgotten ? `Forgot ${forget.trim()}. Its next event starts fresh.` : `Nothing was remembered about ${forget.trim()}.`);
  });
  const copy = (text: string) => void navigator.clipboard?.writeText(text).catch(() => {});
  return <Dialog drawer title={`${system.name} · Events`} close={close} busy={m.busy} status={m.status}
    footer={<Button onClick={close}>Done</Button>}>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading events…</p>}
    {data && <div className="system-events">
      <p className="ui-help">Your system starts {system.name}’s automations by sending one web call per event to this address, with the key as a bearer token. Approvals still happen only inside Cuckoo.</p>
      <dl className="agent-facts"><dt>Address</dt><dd><code>{data.address}</code></dd><dt>Events</dt><dd>{data.events_on ? 'On' : 'Off'}</dd></dl>
      {key && <div className="system-key" role="note"><strong>Events key (shown once)</strong><code>{key}</code>
        <Button onClick={() => copy(key)}>Copy key</Button></div>}
      {canChange && <div className="system-actions">
        <Button primary={!data.events_on} disabled={m.busy} onClick={() => turn(true)}>{data.events_on ? 'Replace the key' : 'Turn on events'}</Button>
        {data.events_on && (confirmOff
          ? <><span>The system can’t start work until events are on again.</span><Button disabled={m.busy} onClick={() => turn(false)}>Turn off</Button><Button onClick={() => setConfirmOff(false)}>Keep on</Button></>
          : <Button disabled={m.busy} onClick={() => setConfirmOff(true)}>Turn off events</Button>)}
      </div>}
      {data.events_on && <pre className="system-sample">{`POST ${data.address}
Authorization: Bearer <events key>
Content-Type: application/json

{"id":"evt-5001","type":"exam.result_published",
 "subject":"stu-104","time":"2026-11-13T09:00:00Z",
 "data":{"exam":"Maths term 1"}}`}</pre>}
      <h3>Recent events</h3>
      {!data.recent.length ? <p className="ui-empty">Nothing has arrived yet.</p> : <div className="system-table"><table>
        <thead><tr><th>Received</th><th>Event</th><th>Subject</th><th>Fields</th><th>What happened</th></tr></thead>
        <tbody>{data.recent.map(event => <tr key={event.id}>
          <td>{new Date(event.received_ms).toLocaleString()}</td><td><code>{event.type}</code>{event.test ? ' (test)' : ''}</td>
          <td>{event.subject ?? '—'}</td><td>{event.fields.join(', ') || '—'}</td>
          <td>{event.matched ? `Accepted · ${event.matched} ${event.matched === 1 ? 'run' : 'runs'}` : 'Matched nothing: no automation for this type'}</td>
        </tr>)}</tbody></table></div>}
      {canChange && data.events_on && <fieldset className="system-test"><legend>Send test event</legend>
        <label className="ui-field">Event type<input value={type} placeholder="exam.result_published" list={`types-${system.id}`} onChange={event => setType(event.target.value)} /></label>
        <datalist id={`types-${system.id}`}>{data.types.map(t => <option key={t.type} value={t.type} />)}</datalist>
        <label className="ui-field">Sample data (JSON)<textarea rows={3} value={sample} onChange={event => setSample(event.target.value)} /></label>
        <Button disabled={m.busy} onClick={test}>Send test event</Button>
      </fieldset>}
      {canChange && <fieldset className="system-test"><legend>Forget a subject</legend>
        <p className="ui-help">Clears what {system.name}’s automations remember about one subject, such as a student who left.</p>
        <label className="ui-field">Subject<input value={forget} placeholder="stu-104" onChange={event => setForget(event.target.value)} /></label>
        <Button disabled={m.busy} onClick={forgetOne}>Forget this subject</Button>
      </fieldset>}
    </div>}
  </Dialog>;
}
