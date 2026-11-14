import { useEffect, useRef, useState } from 'react';
import { identityRequest, storageScope, workspaceSession } from '../../../workspace-context';
import { Button, Dialog } from '../../ui';
import { useMutation } from '../../requests';
import { eventPath, type Events, type EventInput } from './event-model';
import { MemoryChoice, ResultsChoice, memoryLabels, type Memory } from './results-and-memory';
function pending(key: string): EventInput | undefined {
  try {
    const v = JSON.parse(window.sessionStorage.getItem(key) ?? 'null');
    if (
      v &&
      typeof v.id === 'string' &&
      typeof v.source === 'string' &&
      typeof v.binding === 'string' &&
      (v.chat === undefined || typeof v.chat === 'string') &&
      typeof v.name === 'string' &&
      typeof v.instructions === 'string' &&
      v.filter &&
      typeof v.filter === 'object'
    )
      return v;
  } catch {
    /* Only exact locally saved requests can be retried. */
  }
}
export function CreateEvent({
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
  const key = storageScope(`relay.pending-event-trigger.${agent}`),
    [original] = useState(() => pending(key));
  const [selected, select] = useState(original?.source ?? directory.choices[0]?.source.id ?? '');
  const [name, setName] = useState(original?.name ?? ''),
    [task, setTask] = useState(original?.instructions ?? '');
  const [filters, setFilters] = useState<Record<string, string>>(
    () =>
      Object.fromEntries(Object.entries(original?.filter ?? {}).filter(([, v]) => typeof v === 'string')) as Record<
        string,
        string
      >,
  );
  const [review, setReview] = useState<EventInput | undefined>(original);
  const attempt = useRef<EventInput | undefined>(original),
    alive = useRef(false);
  const m = useMutation();
  const [confirmed, setConfirmed] = useState(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const choice = directory.choices.find(
    (c) =>
      c.source.id === (review?.source ?? selected) &&
      (!review ||
        (c.source.binding === review.binding &&
          (!review.chat || (c.conversations ?? []).some((x) => x.id === review.chat)))),
  );
  // Runs happen in a private work room (schema 115); posting a summary is optional.
  const [post, setPost] = useState(Boolean(original?.chat));
  const [destination, setDestination] = useState(original?.chat ?? '');
  const [memory, setMemory] = useState<Memory>(original?.memory ?? 'fresh');
  const places = choice?.conversations ?? [];
  const wanted = review ? review.chat : post ? destination : undefined;
  const place = wanted === undefined ? undefined : (places.find((x) => x.id === wanted) ?? (review ? undefined : places[0]));
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
      if (!choice) throw new Error('Choose a currently reviewed source.');
      if (post && !place) throw new Error('Choose a conversation you may post to, or keep results in Activity only.');
      if (
        !name.trim() ||
        new TextEncoder().encode(name).length > 100 ||
        !task.trim() ||
        new TextEncoder().encode(task).length > 4000
      )
        throw new Error('Use a name up to 100 bytes and task instructions up to 4,000 bytes.');
      const filter = Object.fromEntries(Object.entries(filters).filter(([, v]) => v !== ''));
      if (new TextEncoder().encode(JSON.stringify(filter)).length > 1000)
        throw new Error('Keep exact filters under 1,000 bytes in total.');
      setReview({
        id: crypto.randomUUID(),
        source: choice.source.id,
        binding: choice.source.binding,
        ...(place ? { chat: place.id } : {}),
        ...(memory === 'remember' ? { memory } : {}),
        name,
        instructions: task,
        filter,
        ...choice.authority,
      });
    });
  const submit = () =>
    void m.run(async () => {
      if (!review || !choice || !directory.execution_available)
        throw new Error('Refresh the current source before saving.');
      const exact = attempt.current ?? review;
      attempt.current = exact;
      try {
        window.sessionStorage.setItem(key, JSON.stringify(exact));
      } catch {
        /* Exact in-memory receipt remains available. */
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
  return (
    <Dialog
      drawer
      title="New trigger from a reviewed source"
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
            <Button primary disabled={m.busy || !choice || !directory.execution_available} onClick={submit}>
              {attempt.current ? 'Retry same save' : 'Activate event trigger'}
            </Button>
          </>
        ) : (
          <Button primary disabled={m.busy || !choice} onClick={inspect}>
            Review event trigger
          </Button>
        )
      }
    >
      {confirmed ? (
        <p role="status">Event trigger saved. New matching provider deliveries can now request work.</p>
      ) : review ? (
        <div className="routine-review">
          {attempt.current && (
            <p role="status">
              Retrying this exact save cannot create a second trigger. Check saved triggers before creating a different
              one.
            </p>
          )}
          {!choice && (
            <p role="status">
              The reviewed source or destination is no longer available. Check saved triggers before starting a
              different request.
            </p>
          )}
          {attempt.current && (m.status || !choice) && (
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
            <dt>Event source</dt>
            <dd>{choice?.source.name ?? 'Unavailable'}</dd>
            <dt>Account</dt>
            <dd>{choice?.account ?? choice?.account_name ?? 'Reviewed connected account'}</dd>
            <dt>Included fields</dt>
            <dd>{choice?.source.fields.join(', ')}</dd>
            <dt>Results</dt>
            <dd>{review.chat ? `Activity, and a summary in ${place?.name ?? 'a conversation no longer available'}` : 'Activity only'}</dd>
            <dt>Memory</dt>
            <dd>{memoryLabels[review.memory ?? 'fresh'].title}</dd>
            <dt>Authorized by</dt>
            <dd>{workspaceSession?.user?.display_name ?? workspaceSession?.user?.email ?? 'You'}</dd>
            <dt>Exact text filters</dt>
            <dd>
              {Object.entries(review.filter)
                .map(([k, v]) => `${k} = ${String(v)}`)
                .join('; ') || 'All events from this source'}
            </dd>
          </dl>
          <p>
            Each run happens in its own private room and appears in Activity. It never reads anyone’s conversations.
            {review.chat
              ? ' A short result is posted to the conversation you chose; its current members are the audience.'
              : ' Only you, the agent’s managers and admins see what it found.'}
          </p>
          <p>
            Automatic reading needs the read action set to Allow on the agent’s Tools tab. Every write or other controlled action keeps its own
            approval. Pausing the trigger blocks future intake; accepted running work has a separate Stop.
          </p>
        </div>
      ) : (
        <div className="ui-form">
          <label className="ui-field">
            Event source
            <select
              aria-label="Event source"
              value={selected}
              onChange={(e) => {
                select(e.target.value);
                setFilters({});
              }}
            >
              {directory.choices.map((c) => (
                <option key={c.source.id} value={c.source.id}>
                  {c.source.name}
                </option>
              ))}
            </select>
          </label>
          <ResultsChoice name="event" post={post} setPost={setPost} chat={place?.id ?? places[0]?.id ?? ''}
            setChat={setDestination} conversations={places} />
          <MemoryChoice name="event" value={memory} change={setMemory} />
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
              Leave a field empty to accept any value. Text matching is case-sensitive; all filled filters must match.
            </p>
            {choice?.source.fields.map((field) => (
              <label className="ui-field" key={field}>
                {field}
                <input
                  value={filters[field] ?? ''}
                  onChange={(e) => setFilters({ ...filters, [field]: e.target.value })}
                />
              </label>
            ))}
          </fieldset>
        </div>
      )}
    </Dialog>
  );
}
