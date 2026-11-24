import { useEffect, useState } from 'react';
import { Button, Dialog } from '../ui';
import { AddNote } from './add-note';
import { errorMessage, useMutation } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import { agentSectionUrl } from '../agents/workspace-tabs';
import { clock, closeTask, dayLabel, loadTask, pauseTask, runLabels, skipNextRun, status, steerTask, taskLabels, taskTone, waitingLabels, type TaskDetail } from './model';

const when = (ms: number) => `${dayLabel(ms)} ${clock(ms)}`;

/** One task: what it promises, its status from the records, each request that
 * worked on it, and closing it. "Not a task" is for anyone who can see it; done
 * and cancelled for the person it's for, the agent's owner and admins. */
export function TaskDetails({ id, close }: { id: string; close: () => void }) {
  const [state, setState] = useState<{ data?: TaskDetail; error?: string }>({});
  const [version, setVersion] = useState(0);
  const mutation = useMutation();
  useEffect(() => {
    const controller = new AbortController();
    setState({});
    loadTask(id, controller.signal)
      .then(data => { if (!controller.signal.aborted) setState({ data }); })
      .catch(error => { if (!controller.signal.aborted) setState({ error: errorMessage(error) }); });
    return () => controller.abort();
  }, [id, version]);
  const data = state.data;
  const finish = (outcome: 'done' | 'cancelled' | 'not_task', message: string) => void mutation.run(async () => {
    await closeTask(id, outcome); mutation.setStatus(message); setVersion(v => v + 1);
  });
  const open = data && !data.outcome && !['done', 'failed', 'cancelled'].includes(data.status);
  const life = data?.lifetime;
  const repeating = Boolean(life?.rule_text);
  // An event trigger's task (schema 148) runs when its event arrives: it pauses and
  // ends here, and resumes with its trigger on the agent's page.
  const trigger = data?.trigger;
  const standing = repeating || Boolean(trigger);
  const setPaused = (paused: boolean) => void mutation.run(async () => {
    await pauseTask(id, paused); mutation.setStatus(paused ? 'Task paused.' : 'Task resumed.'); setVersion(v => v + 1);
  });
  const skip = () => void mutation.run(async () => { await skipNextRun(id); mutation.setStatus('Next run skipped.'); setVersion(v => v + 1); });
  return <Dialog drawer title={data ? data.title : 'Task'} close={close} busy={mutation.busy} status={mutation.status}>
    {!data && !state.error && <p role="status">Loading task…</p>}
    {state.error && <p role="alert">{state.error}</p>}
    {data && <section className="activity-details" aria-label="Task details">
      <span className={`activity-status ${taskTone(data.status)}`}>{taskLabels[data.status]}
        {data.status === 'waiting' && data.waiting_on ? ` for ${waitingLabels[data.waiting_on]}` : ''}</span>
      {repeating && <p className="activity-task-rule">{life!.rule_text}</p>}
      {trigger && <p className="activity-task-rule">When {trigger.source ?? 'its event'} sends an event</p>}
      {data.paused_reason && open && <p className="activity-fix" role="note"><strong>Paused:</strong> {data.paused_reason} Nothing runs until it is resumed.</p>}
      <dl className="activity-facts">
        <dt>Agent</dt><dd>{data.agent_name}</dd>
        <dt>For</dt><dd>{data.requester_name ?? 'Former member'}</dd>
        <dt>Where</dt><dd>{data.conversation_kind === 'direct' ? 'direct chat' : `#${data.conversation_name}`}</dd>
        <dt>Started</dt><dd>{when(data.created_ms)}{data.kind === 'automatic' ? ' · tracked automatically' : data.origin === 'person' ? ' · made a task by a person' : data.origin === 'schedule' ? ' · was a schedule' : trigger ? ' · an event trigger' : data.origin === 'check' ? ' · regular checks, set on the assistant page' : ' · promised by the agent'}</dd>
        {trigger && life && <><dt>Runs so far</dt><dd>{life.runs}</dd></>}
        {data.start_ms && <><dt>Starts</dt><dd>{when(data.start_ms)}</dd></>}
        {data.due_ms && <><dt>Due</dt><dd>{when(data.due_ms)}</dd></>}
        {repeating && life!.next_text && <><dt>Next run</dt><dd>{life!.next_text} · {life!.timezone === 'Asia/Kolkata' ? 'India time' : life!.timezone}</dd></>}
        {repeating && <><dt>Runs so far</dt><dd>{life!.runs}{life!.max_runs ? ` of ${life!.max_runs}` : ''}</dd></>}
        {!repeating && data.next_wakeup_ms && <><dt>Next check</dt><dd>{when(data.next_wakeup_ms)} · the agent continues then</dd></>}
        {data.done_when && <><dt>Done when</dt><dd>{data.done_when}</dd></>}
      </dl>
      {data.blocked_wakeup && <p className="activity-fix" role="note"><strong>The agent couldn't continue:</strong> {data.blocked_wakeup.replace(/^Follow-up blocked: /, '')} Fix access, then ask the agent again, or close the task.</p>}
      {data.result && <p className="activity-fix"><strong>Result:</strong> {data.result}</p>}
      {data.runs && data.runs.length > 0 && <>
        <h3>Runs</h3>
        <ol className="activity-steps">{data.runs.map(run => {
          const tone = ['failed', 'blocked', 'missed'].includes(run.state) ? 'problem' : ['done', 'no_change'].includes(run.state) ? 'done' : 'waiting';
          const label = `${run.kind === 'continue' ? 'Next part' : run.run ? `Run ${run.run}` : run.kind === 'follow_up' ? 'Follow-up' : 'Run'} · ${runLabels[run.state]}`;
          return <li key={`${run.at}-${run.kind}`} className={tone}>
            {run.reply ? <a href={workspaceUrl(`/?view=activity&tab=history&run=${encodeURIComponent(run.reply)}`)}><strong>{label}</strong></a> : <strong>{label}</strong>}
            <small>{when(run.at)}{run.detail ? ` · ${run.detail}` : ''}</small>
          </li>;
        })}</ol>
      </>}
      {data.notes && data.notes.length > 0 && <>
        <h3>Notes the agent keeps for this task</h3>
        <ul className="activity-task-notes">{data.notes.map(note => <li key={note.created_ms}><small>{dayLabel(note.created_ms)}</small> {note.note}</li>)}</ul>
      </>}
      {repeating && open && <p className="activity-task-change">To change it, tell {data.agent_name} in chat, for example “make it every 3 days” or “stop after 3 reminders”.</p>}
      <h3>Steps</h3>
      <ol className="activity-steps">{data.requests.map(step => {
        const shown = status(step);
        return <li key={step.id} className={shown.tone === 'problem' ? 'problem' : shown.tone === 'done' ? 'done' : 'waiting'}>
          <a href={workspaceUrl(`/?view=activity&tab=history&run=${encodeURIComponent(step.id)}`)}><strong>{step.summary}</strong></a>
          <small>{when(step.created_ms)} · {shown.label}</small>
        </li>;
      })}</ol>
      {['working', 'waiting'].includes(data.status) && <AddNote label="Add to this task" send={text => steerTask(id, text)} />}
      {open && trigger && data.may_close && <p className="ui-help">Ending this task removes its trigger for good.</p>}
      {open && <div className="activity-detail-actions">
        {data.may_close && repeating && (life!.paused
          ? <Button primary disabled={mutation.busy} onClick={() => setPaused(false)}>Resume</Button>
          : <Button disabled={mutation.busy} onClick={() => setPaused(true)}>Pause</Button>)}
        {data.may_close && trigger && (trigger.state === 'enabled'
          ? <Button disabled={mutation.busy} onClick={() => setPaused(true)}>Pause</Button>
          : <a className="ui-button" href={agentSectionUrl(data.agent, 'Automations')}>Resume on the agent's page</a>)}
        {data.may_close && repeating && !life!.paused && life!.next_ms && <Button disabled={mutation.busy} onClick={skip}>Skip next run</Button>}
        {data.may_close && !standing && <Button primary disabled={mutation.busy} onClick={() => finish('done', 'Marked done.')}>Mark done</Button>}
        {data.may_close && <Button disabled={mutation.busy} onClick={() => finish('cancelled', standing ? 'Task ended.' : 'Task cancelled.')}>{standing ? 'End task' : 'Cancel task'}</Button>}
        {!trigger && <Button disabled={mutation.busy} onClick={() => finish('not_task', 'Marked as not a task.')}>Not a task</Button>}
      </div>}
    </section>}
  </Dialog>;
}
