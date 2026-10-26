import { useCallback, useEffect, useRef } from 'react';
import { identityRequest, workspaceUrl } from '../../../workspace-context';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { subscribeChanges } from '../../chat/live';
import { eventPath, type EventTrigger, type Delivery } from './event-model';
export function EventReview({
  agent,
  trigger,
  close,
  saved,
}: {
  agent: string;
  trigger: EventTrigger;
  close: () => void;
  saved: () => Promise<void>;
}) {
  const path = eventPath(agent, trigger.id);
  const load = useCallback(async () => {
    const [current, runs] = await Promise.all([
      identityRequest<EventTrigger>(path),
      identityRequest<{ runs: Delivery[] }>(`${path}/runs`),
    ]);
    return { current, runs: runs.runs };
  }, [path]);
  const r = useResource(load),
    m = useMutation();
  const alive = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const exact = useRef<{ payload: string; request: string } | undefined>(undefined);
  useEffect(() => {
    const timer = window.setInterval(() => {
      if (!document.hidden) void r.refresh().catch(() => {});
    }, 5000);
    const off = subscribeChanges((c) => {
      if (['access', 'reset'].includes(c.kind) || (c.kind === 'event_access' && c.source === trigger.source)) {
        r.setData(undefined);
        void r.refresh().catch(() => {});
      }
    });
    return () => {
      clearInterval(timer);
      off();
    };
  }, [r.refresh, r.setData, trigger.source]);
  const t = r.data?.current;
  const change = (enabled: boolean) =>
    void m.run(async () => {
      if (!t) throw new Error('Refresh this trigger before changing it.');
      const input = {
        revision: t.revision,
        enabled,
        connection_revision: enabled ? t.choice?.authority.connection_revision : null,
        read_revision: enabled ? t.choice?.authority.read_revision : null,
        delegation_revision: enabled ? t.choice?.authority.delegation_revision : null,
      };
      const payload = JSON.stringify(input);
      if (exact.current?.payload !== payload) exact.current = { payload, request: crypto.randomUUID() };
      await identityRequest(`${path}/state`, { ...input, request: exact.current.request });
      if (alive.current) await Promise.all([r.refresh(), saved()]);
    });
  return (
    <Dialog
      drawer
      title="Review event trigger"
      close={close}
      busy={m.busy}
      status={m.status}
      footer={
        <Button disabled={m.busy} onClick={() => void r.refresh().catch(() => {})}>
          Refresh trigger
        </Button>
      }
    >
      {r.error && <p role="alert">{r.error}</p>}
      {!t && !r.error && <p role="status">Loading saved trigger…</p>}
      {t && (
        <div className="routine-review">
          <h3>{t.name}</h3>
          <p role="status">{t.state}</p>
          <p className="routine-instructions">{t.instructions}</p>
          <dl className="agent-facts">
            <dt>Source</dt>
            <dd>{t.source_definition.name}</dd>
            <dt>Event type</dt>
            <dd>{t.source_definition.event_type}</dd>
            <dt>Included fields</dt>
            <dd>{t.source_definition.fields.join(', ')}</dd>
            <dt>Destination</dt>
            <dd>{t.chat_name}</dd>
            <dt>Authorized by</dt>
            <dd>{t.principal_name}</dd>
            <dt>Exact filters</dt>
            <dd>
              {Object.entries(t.filter)
                .map(([k, v]) => `${k} = ${String(v)}`)
                .join('; ') || 'All source events'}
            </dd>
          </dl>
          {t.detail && <p role="status">{t.detail}</p>}
          {t.state === 'enabled' ? (
            <Button disabled={m.busy} onClick={() => change(false)}>
              Pause event trigger
            </Button>
          ) : (
            <>
              <p className="ui-help">
                Only {t.principal_name} can resume with current source, read and destination authority. Old deliveries
                remain stopped.
              </p>
              <Button primary disabled={m.busy || !t.can_resume} onClick={() => change(true)}>
                Resume event trigger
              </Button>
            </>
          )}
          <p className="ui-help">
            Pause cancels pending and unstarted tasks. Running work and saved continuations keep a separate task Stop.
            To change this source, task or filters, pause and create a new trigger.
          </p>
          <h3>Recent deliveries</h3>
          {!r.data?.runs.length && <p>No matched deliveries saved yet.</p>}
          {r.data?.runs.map((run) => (
            <article className="routine-row" key={run.id}>
              <p>
                {new Date(run.received_ms).toLocaleString()} ·{' '}
                {run.revoked ? 'Source access changed' : (run.task_state ?? run.state)}
              </p>
              {run.progress?.state && <p>{run.progress.state.replaceAll('_', ' ')}</p>}
              {run.detail && <p>{run.detail}</p>}
              {run.reply && !run.revoked && (
                <a
                  href={workspaceUrl(
                    `/?view=chat&conversation=${encodeURIComponent(run.chat)}&message=${encodeURIComponent(run.reply)}`,
                  )}
                >
                  Open original task
                </a>
              )}
            </article>
          ))}
          <p className="ui-help">
            Only matched deliveries appear here. Incoming data is available to the original task while its reviewed
            access remains valid. Listening and reading status use no model calls.
          </p>
        </div>
      )}
    </Dialog>
  );
}
