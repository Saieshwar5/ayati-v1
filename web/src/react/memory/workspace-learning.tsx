import { useCallback } from 'react';
import { useResource } from '../requests';
import { compare, loadLearning } from '../agents/improving/improving-model';
import { WeekTable } from '../agents/improving/week-table';
import '../agents/improving/improving.css';

/** Settings › Memory › Learning, for admins: the workspace's week against the
 * week before, and each agent's corrected answers. */
export function WorkspaceLearning() {
  const { data, error } = useResource(useCallback(() => loadLearning(), []));
  if (error && !data) return <p role="alert">{error}</p>;
  if (!data) return <p role="status">Loading this week’s learning…</p>;
  if (!data.available || !data.this_week || !data.last_week) return null;
  return <details className="improving-group workspace-learning" open>
    <summary>How your agents are learning</summary>
    {!data.configured?.classifier && <p className="ui-help">Answers are judged by reactions and refused approvals only, until a classifier model is set on this Cuckoo.</p>}
    <WeekTable now={data.this_week} before={data.last_week} caption="All agents, this week and last" />
    {(data.agents?.length ?? 0) > 0 && <ul className="improving-list" aria-label="Each agent">
      {data.agents!.map(a => {
        const change = compare(a.this_week.corrected, a.last_week.corrected, true);
        return <li key={a.id}><span><strong>{a.name}</strong></span>
          <small>{a.this_week.answers} answers · corrected {a.this_week.corrected ?? '–'} per 100, last week {a.last_week.corrected ?? '–'}
            {change === 'better' ? ' · better' : change === 'worse' ? ' · worse' : ''}</small></li>;
      })}
    </ul>}
  </details>;
}
