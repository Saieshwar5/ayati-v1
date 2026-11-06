import { useCallback, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { dateLabel, request } from './model';

type Suggestion = {
  id: string;
  base_revision: number;
  text: string;
  note: string;
  created: number;
  agent: string;
  requester: string;
  stale: boolean;
};
type Waiting = { suggestions: Suggestion[]; current: string; editable: boolean };

/** Agents' suggested changes to a page (Library LB3): nothing changes until someone who
 * can edit the page accepts one; a suggestion on an older revision can only be rejected. */
export function Suggestions({
  id,
  revision,
  changed,
}: {
  id: string;
  revision: number;
  changed: () => void;
}) {
  // Reload when the page's revision changes, so a decision elsewhere shows here.
  const load = useCallback(() => request<Waiting>(`/${id}/suggestions`), [id, revision]);
  const waiting = useResource(load);
  const [open, setOpen] = useState(false);
  const count = waiting.data?.suggestions.length ?? 0;
  if (!count) return null;
  return (
    <>
      <p className="library-notice library-suggestion-bar" role="status">
        {count === 1
          ? `${waiting.data!.suggestions[0].agent} suggested a change.`
          : `${count} suggested changes from agents.`}{' '}
        <Button onClick={() => setOpen(true)}>Review</Button>
      </p>
      {open && waiting.data && (
        <ReviewDialog
          id={id}
          waiting={waiting.data}
          close={() => setOpen(false)}
          decided={() => {
            void waiting.refresh().catch(() => {});
            changed();
          }}
        />
      )}
    </>
  );
}

function ReviewDialog({
  id,
  waiting,
  close,
  decided,
}: {
  id: string;
  waiting: Waiting;
  close: () => void;
  decided: () => void;
}) {
  const mutation = useMutation();
  // One request identity per decision, kept across a retry of the same click.
  const attempts = useRef(new Map<string, string>());
  const decide = (suggestion: string, action: 'accept' | 'reject') => {
    const key = `${suggestion}:${action}`;
    if (!attempts.current.has(key)) attempts.current.set(key, crypto.randomUUID());
    void mutation.run(async () => {
      await request(`/${id}/suggestions/${suggestion}`, {
        request: attempts.current.get(key),
        action,
      });
      mutation.setStatus(action === 'accept' ? 'Accepted as a new version.' : 'Rejected.');
      decided();
    });
  };
  return (
    <Dialog
      title="Suggested changes"
      drawer
      close={close}
      busy={mutation.busy}
      status={mutation.status}
    >
      <p>
        Accepting saves the suggested text as a new version, marked with the agent; History can
        restore the earlier one.
      </p>
      {waiting.suggestions.map((s) => (
        <section
          className="library-suggestion"
          key={s.id}
          aria-label={`Suggestion from ${s.agent}`}
        >
          <h3>
            {s.agent}{' '}
            <small>
              for {s.requester} · {dateLabel(s.created)}
            </small>
          </h3>
          {s.note && <p className="library-suggestion-note">“{s.note}”</p>}
          {s.stale && (
            <p className="library-muted">
              The page changed after this was suggested (revision {s.base_revision}). It can only be
              rejected; ask the agent again on the new text.
            </p>
          )}
          <div className="library-compare">
            <div>
              <h4>Now</h4>
              <pre>{waiting.current}</pre>
            </div>
            <div>
              <h4>Suggested</h4>
              <pre>{s.text}</pre>
            </div>
          </div>
          {waiting.editable ? (
            <div className="library-suggestion-actions">
              <Button
                primary
                disabled={mutation.busy || s.stale}
                onClick={() => decide(s.id, 'accept')}
              >
                Accept
              </Button>
              <Button disabled={mutation.busy} onClick={() => decide(s.id, 'reject')}>
                Reject
              </Button>
            </div>
          ) : (
            <p className="library-muted">Someone who can edit this page decides.</p>
          )}
        </section>
      ))}
    </Dialog>
  );
}
