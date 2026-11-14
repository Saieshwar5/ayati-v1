import { useCallback, useEffect, useRef, useState } from 'react';
import { identityRequest, storageScope, workspaceSession } from '../../../workspace-context';
import { Button, Dialog } from '../../ui';
import { useMutation, useResource } from '../../requests';
import { eventPath, loadAppEvents, type AppTriggerInput, type Events } from './event-model';
import { MemoryChoice, ResultsChoice, memoryLabels, type Memory } from './results-and-memory';

function pending(key: string): AppTriggerInput | undefined {
  try {
    const v = JSON.parse(window.sessionStorage.getItem(key) ?? 'null');
    if (
      v &&
      typeof v.id === 'string' &&
      typeof v.connection === 'string' &&
      typeof v.event === 'string' &&
      (v.chat === undefined || typeof v.chat === 'string') &&
      typeof v.name === 'string' &&
      typeof v.instructions === 'string' &&
      v.filter &&
      typeof v.filter === 'object'
    )
      return v;
  } catch {
    /* Only an exact locally saved request can be retried. */
  }
}

const tooLong = (text: string, bytes: number) => new TextEncoder().encode(text).length > bytes;

/** "When something happens in an app": a connected app, one of its events,
 * optional exact filters on its fields, then the usual instructions and
 * destination. The server subscribes at the app's provider on activation. */
export function CreateAppEvent({
  agent,
  directory,
  close,
  saved,
}: {
  agent: string;
  directory: Events;
  close: () => void;
  saved: () => Promise<void>;
}) {
  const apps = directory.apps ?? [];
  const key = storageScope(`relay.pending-app-trigger.${agent}`),
    [original] = useState(() => pending(key));
  const [connection, setConnection] = useState(
    original?.connection ?? (apps.find((a) => a.state === 'ready') ?? apps[0])?.connection ?? '',
  );
  const app = apps.find((a) => a.connection === connection);
  const ready = app?.state === 'ready';
  const load = useCallback(
    async () => (ready ? (await loadAppEvents(connection)).events : []),
    [connection, ready],
  );
  const events = useResource(load);
  const [chosen, choose] = useState(original?.event ?? '');
  const event = events.data?.find((e) => e.id === chosen) ?? events.data?.[0];
  // Runs happen in a private work room (schema 115); posting a summary is optional.
  const [post, setPost] = useState(Boolean(original?.chat));
  const [destination, setDestination] = useState(original?.chat ?? '');
  const place = post ? (app?.conversations.find((c) => c.id === destination) ?? app?.conversations[0]) : undefined;
  const [memory, setMemory] = useState<Memory>(original?.memory ?? 'fresh');
  const [name, setName] = useState(original?.name ?? ''),
    [task, setTask] = useState(original?.instructions ?? '');
  const [filters, setFilters] = useState<Record<string, string>>(
    () =>
      Object.fromEntries(Object.entries(original?.filter ?? {}).filter(([, v]) => typeof v === 'string')) as Record<
        string,
        string
      >,
  );
  const [review, setReview] = useState<AppTriggerInput | undefined>(original);
  const attempt = useRef<AppTriggerInput | undefined>(original),
    alive = useRef(false);
  const m = useMutation();
  const [confirmed, setConfirmed] = useState(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const reviewed = review && app?.connection === review.connection && ready ? app : undefined;
  const editNew = () => {
    attempt.current = undefined;
    try {
      window.sessionStorage.removeItem(key);
    } catch {
      /* Optional retry receipt. */
    }
    setReview(undefined);
    m.setStatus('');
  };
  const inspect = () =>
    void m.run(async () => {
      if (!app || !ready || !event || !app.authority) throw new Error('Choose a connected app that is ready.');
      if (post && !place) throw new Error('Choose a conversation you may post to, or keep results in Activity only.');
      if (!name.trim() || tooLong(name, 100) || !task.trim() || tooLong(task, 4000))
        throw new Error('Use a name up to 100 bytes and task instructions up to 4,000 bytes.');
      const filter = Object.fromEntries(
        Object.entries(filters).filter(([field, v]) => v !== '' && event.fields.includes(field)),
      );
      if (tooLong(JSON.stringify(filter), 1000)) throw new Error('Keep exact filters under 1,000 bytes in total.');
      setReview({
        id: crypto.randomUUID(),
        connection: app.connection,
        event: event.id,
        ...(place ? { chat: place.id } : {}),
        ...(memory === 'remember' ? { memory } : {}),
        name,
        instructions: task,
        filter,
        ...app.authority,
      });
    });
  const submit = () =>
    void m.run(async () => {
      if (!review || !reviewed || !directory.execution_available)
        throw new Error('Refresh the app before saving.');
      const exact = attempt.current ?? review;
      attempt.current = exact;
      try {
        window.sessionStorage.setItem(key, JSON.stringify(exact));
      } catch {
        /* The exact in-memory request remains available. */
      }
      await identityRequest(eventPath(agent), exact);
      try {
        window.sessionStorage.removeItem(key);
      } catch {
        /* Optional local storage. */
      }
      if (!alive.current) return;
      setConfirmed(true);
      await saved();
    });
  const reviewedEvent = events.data?.find((e) => e.id === review?.event);
  const reviewedPlace = reviewed?.conversations.find((c) => c.id === review?.chat);
  return (
    <Dialog
      drawer
      title="New trigger: when something happens in an app"
      close={close}
      busy={m.busy}
      status={m.status}
      footer={
        confirmed ? (
          <Button onClick={close}>Done</Button>
        ) : review ? (
          <>
            <Button disabled={m.busy || Boolean(attempt.current)} onClick={() => setReview(undefined)}>
              Back
            </Button>
            <Button primary disabled={m.busy || !reviewed || !directory.execution_available} onClick={submit}>
              {attempt.current ? 'Retry same save' : 'Activate event trigger'}
            </Button>
          </>
        ) : (
          <Button primary disabled={m.busy || !ready || !event || (post && !place)} onClick={inspect}>
            Review event trigger
          </Button>
        )
      }
    >
      {confirmed ? (
        <p role="status">Event trigger saved. New matching events from the app can now request work.</p>
      ) : review ? (
        <div className="routine-review">
          {attempt.current && (
            <p role="status">
              Retrying this exact save cannot create a second trigger. Check saved triggers before creating a different
              one.
            </p>
          )}
          {!reviewed && (
            <p role="status">
              This app is no longer ready for events. Check saved triggers before starting a different request.
            </p>
          )}
          {attempt.current && (m.status || !reviewed) && (
            <div>
              <p className="ui-help">
                Editing as a new trigger leaves any saved trigger unchanged. Check the saved list first to avoid
                duplicate work.
              </p>
              <Button disabled={m.busy} onClick={editNew}>
                Edit as a new trigger
              </Button>
            </div>
          )}
          <h3>{review.name}</h3>
          <p className="routine-instructions">{review.instructions}</p>
          <dl className="agent-facts">
            <dt>App</dt>
            <dd>{app?.name ?? 'Unavailable'}</dd>
            <dt>Account</dt>
            <dd>{app?.account ?? 'Connected account'}</dd>
            <dt>Event</dt>
            <dd>{reviewedEvent?.title ?? review.event}</dd>
            <dt>Included fields</dt>
            <dd>{reviewedEvent?.fields.join(', ')}</dd>
            <dt>Results</dt>
            <dd>
              {review.chat ? `Activity, and a summary in ${reviewedPlace?.name ?? 'a conversation no longer available'}` : 'Activity only'}
            </dd>
            <dt>Memory</dt>
            <dd>{memoryLabels[review.memory ?? 'fresh'].title}</dd>
            <dt>Authorized by</dt>
            <dd>{workspaceSession?.user?.display_name ?? workspaceSession?.user?.email ?? 'You'}</dd>
            <dt>Exact text filters</dt>
            <dd>
              {Object.entries(review.filter)
                .map(([k, v]) => `${k} = ${String(v)}`)
                .join('; ') || 'Every event of this kind'}
            </dd>
          </dl>
          <p>
            Each run happens in its own private room and appears in Activity. It never reads anyone’s conversations.
            {review.chat
              ? ' A short result is posted to the conversation you chose; its current members are the audience.'
              : ' Only you, the agent’s managers and admins see what it found.'}
          </p>
          <p>
            Activating asks the app to send these events to Cuckoo. Automatic reading needs{' '}
            {app?.read_title ? `“${app.read_title}”` : 'the read action'} set to Allow on the agent’s Tools tab. Every
            write keeps its own approval. Removing the last trigger for this event stops the app sending it.
          </p>
        </div>
      ) : (
        <div className="ui-form">
          {!apps.length && (
            <p className="ui-help">
              No connected app offers events yet. An admin connects apps on Apps; Gmail and Google Calendar can start
              agents when something happens.
            </p>
          )}
          {apps.length > 0 && (
            <label className="ui-field">
              App
              <select
                aria-label="App"
                value={connection}
                onChange={(e) => {
                  setConnection(e.target.value);
                  choose('');
                  setDestination('');
                  setFilters({});
                }}
              >
                {apps.map((a) => (
                  <option key={a.connection} value={a.connection}>
                    {a.account ? `${a.name} · ${a.account}` : a.name}
                    {a.state === 'ready' ? '' : ' (not ready)'}
                  </option>
                ))}
              </select>
            </label>
          )}
          {app && !ready && <p role="status">{app.detail}</p>}
          {ready && events.error && <p role="alert">{events.error}</p>}
          {ready && !events.data && !events.error && <p role="status">Loading the app’s events…</p>}
          {ready && events.data && !events.data.length && <p role="status">This app offers no events yet.</p>}
          {ready && event && (
            <>
              <label className="ui-field">
                Event
                <select
                  aria-label="Event"
                  value={event.id}
                  onChange={(e) => {
                    choose(e.target.value);
                    setFilters({});
                  }}
                >
                  {events.data?.map((e) => (
                    <option key={e.id} value={e.id}>
                      {e.title}
                    </option>
                  ))}
                </select>
              </label>
              <ResultsChoice
                name="app-event"
                post={post}
                setPost={setPost}
                chat={place?.id ?? app?.conversations[0]?.id ?? ''}
                setChat={setDestination}
                conversations={app?.conversations ?? []}
              />
              <MemoryChoice name="app-event" value={memory} change={setMemory} />
              <label className="ui-field">
                Trigger name
                <input value={name} onChange={(e) => setName(e.target.value)} maxLength={100} />
              </label>
              <label className="ui-field">
                Task instructions
                <textarea value={task} onChange={(e) => setTask(e.target.value)} maxLength={4000} rows={5} />
              </label>
              <fieldset>
                <legend>Optional exact text filters</legend>
                <p className="ui-help">
                  Leave a field empty to accept any value. Text matching is case-sensitive; all filled filters must
                  match.
                </p>
                {event.fields.map((field) => (
                  <label className="ui-field" key={field}>
                    {field}
                    <input
                      value={filters[field] ?? ''}
                      onChange={(e) => setFilters({ ...filters, [field]: e.target.value })}
                    />
                  </label>
                ))}
              </fieldset>
            </>
          )}
        </div>
      )}
    </Dialog>
  );
}
