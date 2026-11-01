import { useCallback, useState } from 'react';
import { AgentFace } from '../faces';
import { useLive } from './use-live';
import { TaskDetails } from './task-details';
import { clock, dayLabel, loadTasks, taskLabels, taskTone, waitingLabels, type Scope, type TaskItem, type TaskStatus } from './model';

const groups: [TaskStatus, string][] = [['needs_fixing', 'Needs fixing'], ['late', 'Late'], ['waiting', 'Waiting'],
  ['working', 'Working'], ['open', 'Open'], ['scheduled', 'Scheduled']];

/** Activity → Tasks: everything the agents have taken on and not finished, what
 * needs a person first (people-and-agents-first, Phase 2). */
export function OpenTasks({ scope, search, initial }: { scope: Scope; search: string; initial?: string }) {
  const { data, error } = useLive(useCallback(() => loadTasks(scope), [scope]));
  const [open, setOpen] = useState<string | undefined>(initial);
  const matches = (task: TaskItem) => !search || `${task.title} ${task.agent_name} ${task.conversation_name}`.toLowerCase().includes(search.toLowerCase());
  const items = (data?.items ?? []).filter(matches);
  return <section className="activity-open" aria-label="Open tasks">
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading open tasks…</p>}
    {data && !items.length && <p className="activity-empty">Nothing open. Promises agents make, and work that waits for someone, appear here until they're done.</p>}
    {groups.map(([state, label]) => {
      const group = items.filter(task => task.status === state);
      if (!group.length) return null;
      return <section key={state} aria-label={label}>
        <h2 className="activity-group">{label} · {group.length}</h2>
        <ul className="activity-list">{group.map(task => <li key={task.id} className="activity-row">
          <button type="button" className="activity-row-button" onClick={() => setOpen(task.id)}>
            <time>{task.due_ms ? `Due ${clock(task.due_ms)}` : clock(task.created_ms)}</time>
            <span className="activity-agent"><AgentFace agent={{ id: task.agent, name: task.agent_name, avatar: task.agent_avatar }} className="activity-face" />{task.agent_name}</span>
            <span className="activity-main"><strong>{task.title}</strong>
              <small>{task.requester_name ?? 'Someone'} · {task.conversation_kind === 'direct' ? 'direct chat' : `#${task.conversation_name}`} · {dayLabel(task.created_ms)}
                {task.status === 'waiting' && task.waiting_on ? ` · waiting for ${waitingLabels[task.waiting_on]}` : ''}
                {task.steps > 1 ? ` · ${task.steps} steps` : ''}</small></span>
            <span className={`activity-status ${taskTone(task.status)}`}>{taskLabels[task.status]}</span>
          </button>
        </li>)}</ul>
      </section>;
    })}
    {data?.more && <p className="ui-help">Showing the first 200 open tasks. Narrow with My agents or search.</p>}
    {open && <TaskDetails id={open} close={() => setOpen(undefined)} />}
  </section>;
}
