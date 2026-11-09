import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '../../ui';
import { useResource } from '../../requests';
import { subscribeChanges } from '../../chat/live';
import { CreateEvent } from './event-create';
import { CreateAppEvent } from './app-event-create';
import { EventReview } from './event-review';
import { loadEvents, type AppTrigger, type EventTrigger } from './event-model';
import { workspaceUrl } from '../../../workspace-context';
export function EventTriggers({ agent }: { agent: string }) {
  const load = useCallback(() => loadEvents(agent), [agent]);
  const r = useResource(load);
  const [open, setOpen] = useState<
    { kind: 'create' } | { kind: 'app' } | { kind: 'review'; trigger: EventTrigger }
  >();
  const button = useRef<HTMLButtonElement>(null),
    opener = useRef<HTMLElement | null>(null),
    timer = useRef<number | undefined>(undefined);
  const show = (next: NonNullable<typeof open>) => {
    opener.current = document.activeElement as HTMLElement;
    setOpen(next);
  };
  const close = () => {
    setOpen(undefined);
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
          <p>Triggers start work when something changes in a connected app.</p>
        </div>
        <div>
          <Button
            ref={button}
            disabled={!r.data?.execution_available || Boolean(r.error)}
            onClick={() => show({ kind: 'app' })}
          >
            When something happens in an app
          </Button>
          <Button
            disabled={!r.data?.execution_available || !r.data.choices.length || Boolean(r.error)}
            onClick={() => show({ kind: 'create' })}
          >
            Create event trigger
          </Button>
        </div>
      </header>
      {r.error && <p role="alert">{r.error}</p>}
      {r.data?.catalog_error && <p role="status">{r.data.catalog_error}</p>}
      {!r.data && !r.error && <p role="status">Loading connected events…</p>}
      {r.data && !r.data.execution_available && (
        <p role="status">Event triggers need controller-managed background work, which this server doesn’t run.</p>
      )}
      {r.data && !r.data.apps?.length && (
        <p className="ui-help">
          No connected app offers events for this agent yet. Once an admin connects Gmail or Google Calendar, its new
          emails or events can start work here.
        </p>
      )}
      {r.data?.apps
        ?.filter((a) => a.state !== 'ready')
        .map((a) => (
          <p key={a.connection} role="status">
            {a.name}: {a.detail}
          </p>
        ))}
      {r.data && !r.data.choices.length && (
        <p className="ui-help">
          No reviewed event source is available to you. An operator must configure the provider source; an admin must
          approve its account and automatic read access in an existing conversation. Event listening uses no model
          calls.
        </p>
      )}
      {r.data?.triggers.map((t) => (
        <article className="routine-row" key={t.id} aria-label={`Trigger: ${t.name}`}>
          <header>
            <h4>{t.name}</h4>
            <span>{t.state === 'enabled' ? 'On' : t.state}</span>
          </header>
          <p>
            {t.source_definition.name} · {t.chat_name}
          </p>
          <p className="routine-instructions">
            {t.instructions}
            {t.instructions_long && '…'}
          </p>
          <p>Set up by {t.principal_name}</p>
          {t.detail && <p role="status">{t.detail}</p>}
          <Button onClick={() => show({ kind: 'review', trigger: t })}>Review trigger and runs</Button>
        </article>
      ))}
      {r.data?.app_triggers?.map((t) => <AppTriggerRow key={`${t.app}/${t.collection}/${t.kind}`} trigger={t} />)}
      {r.data && !r.data.triggers.length && !r.data.app_triggers?.length && <p className="ui-help">No triggers yet.</p>}
      <div className="automation-tools">
        <Button onClick={() => void r.refresh().catch(() => {})}>Refresh triggers</Button>
      </div>
      <p className="ui-help">
        Source data belongs to its original task. Each tool action keeps its own Allow, Ask or Block rule. Provider
        delivery and background workers must be configured separately.
      </p>
      {open?.kind === 'app' && r.data && (
        <CreateAppEvent agent={agent} directory={r.data} close={close} saved={r.refresh} />
      )}
      {open?.kind === 'create' && r.data && (
        <CreateEvent agent={agent} directory={r.data} close={close} saved={r.refresh} />
      )}
      {open?.kind === 'review' && (
        <EventReview key={open.trigger.id} agent={agent} trigger={open.trigger} close={close} saved={r.refresh} />
      )}
    </section>
  );
}

/** A trigger set up from an app (Apps › When something happens). It is listed with the
 * agent's other triggers, and its builder or an admin changes it in the app. */
function AppTriggerRow({ trigger: t }: { trigger: AppTrigger }) {
  const name = `${t.kind === 'added' ? 'New' : 'Changed'} ${t.collection} in ${t.app_name}`;
  return (
    <article className="routine-row" aria-label={`Trigger: ${name}`}>
      <header>
        <h4>{name}</h4>
        <span className="ui-badge">Added from {t.app_name}</span>
      </header>
      <p className="routine-instructions">{t.instructions}</p>
      {t.by && <p>Set up by {t.by}</p>}
      <p className="ui-help">
        {t.manages ? 'Change or stop it' : 'Its builder or an admin changes it'} on{' '}
        <a href={workspaceUrl('/?view=apps')}>Apps › {t.app_name}</a>.
      </p>
    </article>
  );
}
