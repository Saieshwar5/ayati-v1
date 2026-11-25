import { useCallback, useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../../ui';
import { TriggerKind } from './new-automation';
import { useResource } from '../../requests';
import { subscribeChanges } from '../../chat/live';
import { CreateEvent } from './event-create';
import { CreateAppEvent } from './app-event-create';
import { EventReview } from './event-review';
import { loadEvents, type EventTrigger } from './event-model';
import { taskUrl } from '../../activity/model';
import { runsInText } from './runs-in';
/** "When something happens": the agent's triggers. `start` opens a new one from
 * "New automation"; `finished` is told when that dialog closes. */
export function EventTriggers({ agent, agentName, start = false, finished }: { agent: string; agentName: string; start?: boolean; finished?: () => void }) {
  const load = useCallback(() => loadEvents(agent), [agent]);
  const r = useResource(load);
  const [chosen, setOpen] = useState<
    { kind: 'create' } | { kind: 'app' } | { kind: 'review'; trigger: EventTrigger }
  >();
  // A new trigger: straight to apps, or first ask which kind when sources exist too.
  const starting = start && r.data
    ? !r.data.execution_available ? 'unavailable' as const : r.data.choices.length ? 'pick' as const : 'app' as const
    : undefined;
  const open = chosen ?? (starting === 'app' ? { kind: 'app' as const } : undefined);
  const button = useRef<HTMLButtonElement>(null),
    opener = useRef<HTMLElement | null>(null),
    timer = useRef<number | undefined>(undefined);
  const show = (next: NonNullable<typeof open>) => {
    opener.current = document.activeElement as HTMLElement;
    setOpen(next);
  };
  const close = () => {
    setOpen(undefined);
    finished?.();
    timer.current = window.setTimeout(() => {
      if (!opener.current?.isConnected) button.current?.focus();
    }, 0);
  };
  useEffect(() => {
    const interval = window.setInterval(() => {
      if (!document.hidden) void r.refresh().catch(() => {});
    }, 5000);
    const off = subscribeChanges((c) => {
      if (['access', 'reset'].includes(c.kind)) {
        setOpen(undefined);
        r.setData(undefined);
        void r.refresh().catch(() => {});
      } else if (c.kind === 'event_access') void r.refresh().catch(() => {});
    });
    return () => {
      clearInterval(interval);
      clearTimeout(timer.current);
      off();
    };
  }, [r.refresh, r.setData]);
  return (
    <section className="routine-triggers automation-group" aria-label="When something happens">
      <header className="agent-section-heading">
        <div>
          <h4>When something happens</h4>
        </div>
      </header>
      {r.error && <p role="alert">{r.error}</p>}
      {r.data?.catalog_error && <p role="status">{r.data.catalog_error}</p>}
      {!r.data && !r.error && <p role="status">Loading connected events…</p>}
      {r.data && !r.data.execution_available && (
        <p role="status">Triggers can’t run on this server.</p>
      )}
      {r.data && !r.data.apps?.length && (
        <p className="ui-help">No connected app offers events yet.</p>
      )}
      {r.data?.apps
        ?.filter((a) => a.state !== 'ready')
        .map((a) => (
          <p key={a.connection} role="status">
            {a.name}: {a.detail}
          </p>
        ))}
      {r.data && !r.data.choices.length && (
        <p className="ui-help">No reviewed event source is available to you. An operator sets one up and an admin approves it.</p>
      )}
      {r.data?.triggers.map((t) => (
        <article className="routine-row" key={t.id} aria-label={`Trigger: ${t.name}`}>
          <header>
            <h4>{t.name}</h4>
            <span>{t.state === 'enabled' ? 'On' : t.state}</span>
          </header>
          <p>
            {t.source_definition.name} · Runs in {runsInText(agentName, t.chat_name, t.chat_direct)}
          </p>
          <p className="routine-instructions">
            {t.instructions}
            {t.instructions_long && '…'}
          </p>
          <p>Set up by {t.principal_name}</p>
          {t.detail && <p role="status">{t.detail}</p>}
          <div className="automation-tools">
            <Button onClick={() => show({ kind: 'review', trigger: t })}>Review trigger and runs</Button>
            {t.task && <a className="ui-button" href={taskUrl(t.task)}>Open its task</a>}
          </div>
        </article>
      ))}
      {r.data && !r.data.triggers.length && <p className="ui-help">No triggers yet.</p>}
      <div className="automation-tools">
        <Button ref={button} onClick={() => void r.refresh().catch(() => {})}>Refresh triggers</Button>
      </div>
      {!chosen && starting === 'pick' && (
        <TriggerKind close={close} choose={(kind) => setOpen({ kind })} />
      )}
      {!chosen && starting === 'unavailable' && (
        <Dialog title="When something happens" close={close} footer={<Button onClick={close}>Done</Button>}>
          <p>Triggers need background work that this server doesn’t run, so none can be made here.</p>
        </Dialog>
      )}
      {open?.kind === 'app' && r.data && (
        <CreateAppEvent agent={agent} agentName={agentName} directory={r.data} close={close} saved={r.refresh} />
      )}
      {open?.kind === 'create' && r.data && (
        <CreateEvent agent={agent} agentName={agentName} directory={r.data} close={close} saved={r.refresh} />
      )}
      {open?.kind === 'review' && (
        <EventReview key={open.trigger.id} agent={agent} agentName={agentName} trigger={open.trigger} close={close} saved={r.refresh} />
      )}
    </section>
  );
}
