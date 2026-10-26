import { useCallback, useEffect, useRef, useState } from 'react';
import { Button } from '../../ui';
import { useResource } from '../../requests';
import { subscribeChanges } from '../../chat/live';
import { CreateEvent } from './event-create';
import { EventReview } from './event-review';
import { loadEvents, type EventTrigger } from './event-model';
export function EventTriggers({ agent }: { agent: string }) {
  const load = useCallback(() => loadEvents(agent), [agent]);
  const r = useResource(load);
  const [open, setOpen] = useState<{ kind: 'create' } | { kind: 'review'; trigger: EventTrigger }>();
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
    <section className="routine-triggers" aria-label="Connected event triggers">
      <header className="agent-section-heading">
        <div>
          <h3>Event triggers</h3>
          <p>Start work when a reviewed connected system reports a change.</p>
        </div>
        <Button
          ref={button}
          disabled={!r.data?.execution_available || !r.data.choices.length || Boolean(r.error)}
          onClick={() => show({ kind: 'create' })}
        >
          Create event trigger
        </Button>
      </header>
      {r.error && <p role="alert">{r.error}</p>}
      {r.data?.catalog_error && <p role="status">{r.data.catalog_error}</p>}
      {!r.data && !r.error && <p role="status">Loading connected events…</p>}
      {r.data && !r.data.choices.length && (
        <p className="ui-help">
          No reviewed event source is available to you. An operator must configure the provider source; an Owner must
          approve its account and automatic read access in an existing conversation. Event listening uses no model
          calls.
        </p>
      )}
      <Button onClick={() => void r.refresh().catch(() => {})}>Refresh event triggers</Button>
      {r.data?.triggers.map((t) => (
        <article className="routine-row" key={t.id} aria-label={`Event trigger: ${t.name}`}>
          <header>
            <h4>{t.name}</h4>
            <span>{t.state}</span>
          </header>
          <p>
            {t.source_definition.name} · {t.chat_name}
          </p>
          <p className="routine-instructions">
            {t.instructions}
            {t.instructions_long && '…'}
          </p>
          <p>Authorized by {t.principal_name}</p>
          {t.detail && <p role="status">{t.detail}</p>}
          <Button onClick={() => show({ kind: 'review', trigger: t })}>Review trigger and deliveries</Button>
        </article>
      ))}
      {r.data && !r.data.triggers.length && <p>No saved event triggers you may view.</p>}
      <p className="ui-help">
        Source data belongs to its original task. Each tool action keeps its own Allow, Ask or Block rule. Provider
        delivery and background workers must be configured separately.
      </p>
      {open?.kind === 'create' && r.data && (
        <CreateEvent agent={agent} directory={r.data} close={close} saved={r.refresh} />
      )}
      {open?.kind === 'review' && (
        <EventReview key={open.trigger.id} agent={agent} trigger={open.trigger} close={close} saved={r.refresh} />
      )}
    </section>
  );
}
