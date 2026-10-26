import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { chatRequest } from './model';
import './handoffs.css';
export type Handoff = {
  id: string;
  parent: string;
  root: string;
  child: string;
  request_message: string;
  sender_name: string;
  target_name: string;
  principal_name: string;
  chat: string;
  thread: string;
  task: string;
  state: string;
  detail: string;
  expires_ms?: number;
  task_state: string;
  job_state: string;
  phase?: string | null;
  can_stop: boolean;
};
export type Coordination = { incoming: Handoff | null; outgoing: Handoff[] };
export const taskLink = (h: Handoff, original = false) =>
  workspaceUrl(
    `/?view=chat&conversation=${encodeURIComponent(h.chat)}&message=${encodeURIComponent(original ? h.root : h.child)}${original ? '' : `&thread=${encodeURIComponent(h.thread)}`}`,
  );
export function HandoffCard({ handoff, refresh }: { handoff: Handoff; refresh?: () => Promise<void> }) {
  const m = useMutation();
  const state =
    handoff.state !== 'pending'
      ? ({
          complete: 'Completed',
          failed: 'Could not complete',
          cancelled: 'Cancelled',
          expired: 'Wait deadline expired',
        }[handoff.state] ?? handoff.state)
      : handoff.job_state === 'waiting'
        ? ({
            input: 'Waiting for an answer',
            approval: 'Waiting for approval',
            handoff: 'Waiting for another helper',
            attention: 'Needs attention',
          }[handoff.phase ?? ''] ?? 'Waiting')
        : handoff.job_state === 'running'
          ? 'Working'
          : 'Queued';
  return (
    <section className="channel-handoff" aria-label={`Handoff to ${handoff.target_name}`}>
      <header>
        <strong>
          {handoff.sender_name} → {handoff.target_name}
        </strong>
        <span>Agent handoff · {state}</span>
      </header>
      <p>
        {[...handoff.task].slice(0, 180).join('')}
        {[...handoff.task].length > 180 && '…'}
      </p>
      <p className="ui-help">
        Requested by {handoff.principal_name}. {handoff.target_name} keeps its own permissions and {handoff.sender_name}
        's limits.
      </p>
      {handoff.state === 'pending' && handoff.expires_ms && (
        <p className="ui-help">Helper wait deadline: {new Date(handoff.expires_ms).toLocaleString()}.</p>
      )}
      {handoff.detail && <p role="status">{handoff.detail}</p>}
      <div className="handoff-actions">
        <a className="ui-button" href={taskLink(handoff)}>
          Open helper task
        </a>
        <a href={taskLink(handoff, true)}>Open original task</a>
        {handoff.can_stop && (
          <Button
            disabled={m.busy}
            onClick={() =>
              void m.run(async () => {
                await chatRequest(`/chats/${handoff.chat}/messages/${handoff.child}/cancel`, {});
                await refresh?.();
              })
            }
          >
            {m.busy ? 'Stopping helper…' : 'Stop helper task'}
          </Button>
        )}
      </div>
      {m.status && <p role="alert">{m.status}</p>}
    </section>
  );
}
export function CoordinationView({ data, refresh }: { data?: Coordination; refresh?: () => Promise<void> }) {
  if (!data) return null;
  return (
    <>
      {data.incoming && (
        <p className="handoff-origin">
          Helping {data.incoming.sender_name} · Requested by {data.incoming.principal_name}.{' '}
          <a href={taskLink(data.incoming, true)}>Open original task</a>
        </p>
      )}
      {data.outgoing.map((h) => (
        <HandoffCard key={h.id} handoff={h} refresh={refresh} />
      ))}
    </>
  );
}
