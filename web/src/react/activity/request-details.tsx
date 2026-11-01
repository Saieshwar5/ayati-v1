import { useEffect, useState } from 'react';
import './activity.css';
import { Button, Dialog } from '../ui';
import { errorMessage, useMutation } from '../requests';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { cadence, changeSchedule } from '../agents/automation/model';
import { conversationUrl, dayLabel, clock, loadDetails, status, stopRequest, where, type Details } from './model';

/** One request, step by step from Cuckoo's own records (designs/activity 4).
 * Work that is still running can be stopped here, as from the conversation. */
export function RequestDetails({ reply, close }: { reply: string; close: () => void }) {
  const [state, setState] = useState<{ data?: Details; error?: string }>({});
  const [reviewing, review] = useState<string>();
  const [version, setVersion] = useState(0);
  const mutation = useMutation();
  useEffect(() => {
    const controller = new AbortController();
    setState({});
    loadDetails(reply, controller.signal)
      .then(data => { if (!controller.signal.aborted) setState({ data }); })
      .catch(error => { if (!controller.signal.aborted) setState({ error: errorMessage(error) }); });
    return () => controller.abort();
  }, [reply, version]);
  const data = state.data;
  const shown = data && status(data);
  const stop = (item: Details) => void mutation.run(async () => { await stopRequest(item); mutation.setStatus('Stop requested.'); setVersion(v => v + 1); });
  const pause = (item: Details) => void mutation.run(async () => {
    const schedule = item.schedule!;
    await changeSchedule(schedule.agent, schedule.id, { request: crypto.randomUUID(), revision: schedule.revision, enabled: false });
    mutation.setStatus('Schedule paused. Resume it from the agent\'s Schedules.'); setVersion(v => v + 1);
  });
  // A run in its schedule's own room has no conversation to open.
  const room = Boolean(data?.schedule?.room);
  return <Dialog drawer title={data ? data.summary : 'Request'} close={close} busy={mutation.busy} status={mutation.status}>
    {!data && !state.error && <p role="status">Loading request…</p>}
    {state.error && <p role="alert">{state.error}</p>}
    {data && shown && <section className="activity-details" aria-label="Request details">
      <span className={`activity-status ${shown.tone}`}>{shown.label}</span>
      <dl className="activity-facts">
        <dt>Agent</dt><dd>{data.agent_name}</dd>
        {data.schedule ? <>
          <dt>Started by</dt><dd>Schedule · {data.schedule.name} · {cadence(data.schedule.interval_seconds)}</dd>
          <dt>Set up by</dt><dd>{data.requester_name ?? 'Former member'}</dd>
          <dt>Results</dt><dd>{data.schedule.post_to ? `Activity, and a summary in ${data.schedule.post_to}` : 'Activity only'}
            {data.schedule.posted === 'posted' && ' · Posted'}{data.schedule.posted === 'skipped' && ` · Not posted: ${data.schedule.post_reason ?? ''}`}</dd>
        </> : <>
          <dt>Asked by</dt><dd>{data.requester_name ?? 'Former member'}</dd>
          <dt>Where</dt><dd>{where(data)}</dd>
        </>}
        <dt>When</dt><dd>{dayLabel(data.created_ms)} {clock(data.created_ms)}{data.finished_ms ? ` · ${Math.max(1, Math.round((data.finished_ms - data.created_ms) / 1000))} s` : ''}</dd>
      </dl>
      {!data.in_conversation && <p className="ui-help">You are not in this conversation. As a manager or admin you can read this request; your view is recorded in the audit log.</p>}
      <h3>What {data.agent_name} did</h3>
      <ol className="activity-steps">
        <li className="done"><strong>Read the message</strong>{data.message && <blockquote>{data.message}</blockquote>}</li>
        {data.steps.map(step => <li key={step.id} className={step.state === 'completed' ? 'done' : ['denied', 'rejected', 'invalidated', 'uncertain'].includes(step.state) ? 'problem' : 'waiting'}>
          <strong>{step.account ? `${step.account}: ` : ''}{step.title}</strong>
          <small>{step.state === 'denied' ? 'Blocked by its rules' : step.state === 'uncertain' ? 'Outcome unconfirmed: check the business system'
            : step.automatic ? 'Allowed automatically' : step.decided_by ? `${step.decision === 'approve' ? 'Approved' : 'Decided'} by ${step.decided_by}` : 'Waiting for approval'}</small>
          {Object.keys(step.arguments ?? {}).length > 0 && <dl className="activity-fields">{Object.entries(step.arguments).map(([key, value]) =>
            <div key={key}><dt>{key}</dt><dd>{typeof value === 'string' ? value : JSON.stringify(value)}</dd></div>)}</dl>}
          {step.state === 'pending' && <Button onClick={() => review(step.id)}>Review</Button>}
        </li>)}
        {data.reply && <li className={shown.tone === 'problem' ? 'problem' : 'done'}><strong>{shown.tone === 'problem' ? 'Stopped and replied' : 'Replied'}</strong><blockquote>{data.reply}</blockquote></li>}
      </ol>
      {data.disagreement && <p className="activity-warning" role="note">{data.disagreement}</p>}
      {data.fix && <section className="activity-fix" aria-label="How to fix it"><h3>How to fix it</h3><p>{data.fix}</p></section>}
      <div className="activity-detail-actions">
        {data.can_cancel && <Button disabled={mutation.busy} onClick={() => stop(data)}>Stop request</Button>}
        {data.schedule?.state === 'enabled' && <Button disabled={mutation.busy} onClick={() => pause(data)}>Pause schedule</Button>}
        {data.in_conversation && !room && <a className="ui-button" href={conversationUrl(data)}>Open conversation</a>}
      </div>
    </section>}
    {reviewing && <ApprovalDialog id={reviewing} close={() => review(undefined)} />}
  </Dialog>;
}
