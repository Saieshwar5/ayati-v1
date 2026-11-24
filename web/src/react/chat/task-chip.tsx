import './task-progress.css';
import { useState, type ReactNode, type Ref } from 'react';
import { closeTask, taskLabels, taskUrl, type TaskRef } from '../activity/model';
import { useMutation } from '../requests';
import { workspaceSession } from '../../workspace-context';

/** The task an agent reply works towards: one small link to it in Activity → Tasks.
 * A promise shows as "Task"; work the platform tracked shows only once it needs a
 * look or is done, so routine replies stay quiet. */
export function TaskChip({ task }: { task: TaskRef }) {
  if (task.kind === 'automatic' && !['done', 'failed', 'needs_fixing', 'waiting'].includes(task.status)) return null;
  if (task.repeats || task.next_text) return <RepeatingTaskLine task={task} />;
  const tone = ['failed', 'needs_fixing', 'late'].includes(task.status) ? ' problem' : task.status === 'done' ? ' done' : '';
  return <a className={`task-chip${tone}`} href={taskUrl(task.id)}>
    <span aria-hidden="true">◎</span> {task.kind === 'commitment' ? 'Task' : 'Tracked'}: {task.title} · {taskLabels[task.status]} <span aria-hidden="true">→</span>
  </a>;
}

/** A task the agent made in chat that runs on its own (designs/agent-tasks): its
 * name, its next run, View, and Undo for the person it is for. Undo cancels it. */
function RepeatingTaskLine({ task }: { task: TaskRef }) {
  const [undone, setUndone] = useState(false);
  const mutation = useMutation();
  const open = !undone && !['done', 'failed', 'cancelled', 'not_task'].includes(task.status);
  const mine = workspaceSession?.user?.id === task.requester;
  const when = !open ? (undone ? 'undone' : taskLabels[task.status].toLowerCase())
    : task.paused || task.status === 'paused' ? 'paused' : task.next_text ? `next ${task.next_text}` : taskLabels[task.status].toLowerCase();
  const undo = () => void mutation.run(async () => { await closeTask(task.id, 'cancelled'); setUndone(true); mutation.setStatus('Task undone.'); });
  return <div className="task-line" role="group" aria-label={`Task: ${task.title}`}>
    <span aria-hidden="true">◷</span>
    <span className="task-line-name"><strong>{task.title}</strong> · {when}</span>
    <a className="task-line-button" href={taskUrl(task.id)}>View</a>
    {open && mine && <button type="button" className="task-line-button" disabled={mutation.busy} onClick={undo}>Undo</button>}
    {mutation.status && <span className="task-line-status" role="status">{mutation.status}</span>}
  </div>;
}

/** A follow-up the agent booked for itself: one light line in the conversation,
 * so it doesn't read as if the person had asked again. */
export function FollowUpLine({ element, id, task, time }: { element: Ref<HTMLElement>; id: string; task: TaskRef; time: ReactNode }) {
  return <article ref={element} tabIndex={-1} data-message-id={id} className="chat-follow-up" aria-label="Agent follow-up">
    <p><span aria-hidden="true">↻</span> Following up on <a href={taskUrl(task.id)}>{task.title}</a> {time}</p>
  </article>;
}
