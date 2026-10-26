import { useCallback } from 'react';
import { Button, Dialog } from '../../ui';
import { useResource } from '../../requests';
import { requestLabels } from '../request-model';
import { date, loadRuns, runUrl, type Routine } from './model';
export function ScheduleRuns({ agent, routine, close }: { agent: string; routine: Routine; close: () => void }) {
  const load = useCallback(() => loadRuns(agent, routine.id), [agent, routine.id]);
  const resource = useResource(load);
  return <Dialog drawer title={`${routine.name} · Runs`} close={close} footer={<Button onClick={() => void resource.refresh().catch(() => {})}>Refresh runs</Button>}>
    <p>Last 20 accepted occurrences in {routine.chat_name}. An accepted occurrence is not proof the task succeeded.</p>
    {resource.error && <p role="alert">{resource.error}</p>}{!resource.data && !resource.error && <p role="status">Loading saved runs…</p>}
    {resource.data?.runs.map(run => <article className="routine-run" key={run.reply}>
      <strong>{date(run.due_ms, routine.timezone)}</strong><p>{run.state === 'queued' ? requestLabels[run.progress?.state ?? ''] ?? (run.job_state === 'running' ? 'Working' : run.job_state === 'waiting' ? 'Waiting for review' : 'Queued') : requestLabels[run.state] ?? run.state}</p>
      <a className="ui-button" href={runUrl(run)}>Open run in Chat</a>
    </article>)}
    {resource.data && !resource.data.runs.length && <p>No occurrences have been accepted yet.</p>}
  </Dialog>;
}
