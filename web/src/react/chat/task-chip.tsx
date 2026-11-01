import './task-progress.css';
import type { ReactNode, Ref } from 'react';
import { taskLabels, taskUrl, type TaskRef } from '../activity/model';

/** The task an agent reply works towards: one small link to it in Activity → Tasks.
 * A promise shows as "Task"; work the platform tracked shows only once it needs a
 * look or is done, so routine replies stay quiet. */
export function TaskChip({ task }: { task: TaskRef }) {
  if (task.kind === 'automatic' && !['done', 'failed', 'needs_fixing', 'waiting'].includes(task.status)) return null;
  const tone = ['failed', 'needs_fixing', 'late'].includes(task.status) ? ' problem' : task.status === 'done' ? ' done' : '';
  return <a className={`task-chip${tone}`} href={taskUrl(task.id)}>
    <span aria-hidden="true">◎</span> {task.kind === 'commitment' ? 'Task' : 'Tracked'}: {task.title} · {taskLabels[task.status]} <span aria-hidden="true">→</span>
  </a>;
}

/** A follow-up the agent booked for itself: one light line in the conversation,
 * so it doesn't read as if the person had asked again. */
export function FollowUpLine({ element, id, task, time }: { element: Ref<HTMLElement>; id: string; task: TaskRef; time: ReactNode }) {
  return <article ref={element} tabIndex={-1} data-message-id={id} className="chat-follow-up" aria-label="Agent follow-up">
    <p><span aria-hidden="true">↻</span> Following up on <a href={taskUrl(task.id)}>{task.title}</a> {time}</p>
  </article>;
}
