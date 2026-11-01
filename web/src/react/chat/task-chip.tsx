import './task-progress.css';
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
