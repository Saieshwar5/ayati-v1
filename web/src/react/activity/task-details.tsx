import { useEffect, useState } from 'react';
import { Button, Dialog } from '../ui';
import { AddNote } from './add-note';
import { errorMessage, useMutation } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import { clock, closeTask, dayLabel, loadTask, status, steerTask, taskLabels, taskTone, waitingLabels, type TaskDetail } from './model';

const when = (ms: number) => `${dayLabel(ms)} ${clock(ms)}`;

/** One task: what it promises, its status from the records, each request that
 * worked on it, and closing it. "Not a task" is for anyone who can see it; done
 * and cancelled for the person it's for, the agent's managers and admins. */
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
  return <Dialog drawer title={data ? data.title : 'Task'} close={close} busy={mutation.busy} status={mutation.status}>
    {!data && !state.error && <p role="status">Loading task…</p>}
    {state.error && <p role="alert">{state.error}</p>}
    {data && <section className="activity-details" aria-label="Task details">
      <span className={`activity-status ${taskTone(data.status)}`}>{taskLabels[data.status]}
        {data.status === 'waiting' && data.waiting_on ? ` for ${waitingLabels[data.waiting_on]}` : ''}</span>
      <dl className="activity-facts">
        <dt>Agent</dt><dd>{data.agent_name}</dd>
        <dt>For</dt><dd>{data.requester_name ?? 'Former member'}</dd>
        <dt>Where</dt><dd>{data.conversation_kind === 'direct' ? 'direct chat' : `#${data.conversation_name}`}</dd>
        <dt>Started</dt><dd>{when(data.created_ms)}{data.kind === 'automatic' ? ' · tracked automatically' : data.origin === 'person' ? ' · made a task by a person' : ' · promised by the agent'}</dd>
        {data.start_ms && <><dt>Starts</dt><dd>{when(data.start_ms)}</dd></>}
        {data.due_ms && <><dt>Due</dt><dd>{when(data.due_ms)}</dd></>}
        {data.next_wakeup_ms && <><dt>Next check</dt><dd>{when(data.next_wakeup_ms)} · the agent continues then</dd></>}
        {data.done_when && <><dt>Done when</dt><dd>{data.done_when}</dd></>}
      </dl>
      {data.blocked_wakeup && <p className="activity-fix" role="note"><strong>The agent couldn't continue:</strong> {data.blocked_wakeup.replace(/^Follow-up blocked: /, '')} Fix access, then ask the agent again, or close the task.</p>}
      {data.result && <p className="activity-fix"><strong>Result:</strong> {data.result}</p>}
      <h3>Steps</h3>
      <ol className="activity-steps">{data.requests.map(step => {
        const shown = status(step);
        return <li key={step.id} className={shown.tone === 'problem' ? 'problem' : shown.tone === 'done' ? 'done' : 'waiting'}>
          <a href={workspaceUrl(`/?view=activity&tab=history&run=${encodeURIComponent(step.id)}`)}><strong>{step.summary}</strong></a>
          <small>{when(step.created_ms)} · {shown.label}</small>
        </li>;
      })}</ol>
      {['working', 'waiting'].includes(data.status) && <AddNote label="Add to this task" send={text => steerTask(id, text)} />}
      {open && <div className="activity-detail-actions">
        {data.may_close && <Button primary disabled={mutation.busy} onClick={() => finish('done', 'Marked done.')}>Mark done</Button>}
        {data.may_close && <Button disabled={mutation.busy} onClick={() => finish('cancelled', 'Task cancelled.')}>Cancel task</Button>}
        <Button disabled={mutation.busy} onClick={() => finish('not_task', 'Marked as not a task.')}>Not a task</Button>
      </div>}
    </section>}
  </Dialog>;
}
