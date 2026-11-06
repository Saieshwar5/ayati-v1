import { useCallback, useRef } from 'react';
import { navigate } from '../../navigation';
import { identityRequest, workspaceId } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { dateLabel, emptyDocument, itemUrl, request, type Item } from './model';

type ReviewInfo = {
  review: { every_days: number; reviewed: number; due: number; overdue: boolean } | null;
};
const intervals = [30, 90, 180, 365];

/** How often an item is reviewed (LB7): past it, people and agents see it as due. */
export function ReviewControl({ item }: { item: Item }) {
  const load = useCallback(() => request<ReviewInfo>(`/${item.id}/review`), [item.id]);
  const info = useResource(load);
  const mutation = useMutation();
  const review = info.data?.review;
  const set = (body: { every_days: number | null; reviewed: boolean }) =>
    void mutation.run(async () => {
      await request(`/${item.id}/review`, body);
      await info.refresh();
    });
  if (!info.data) return null;
  return (
    <div
      className={review?.overdue ? 'library-notice library-review overdue' : 'library-review'}
      role={review?.overdue ? 'status' : undefined}
    >
      <span>
        {review
          ? review.overdue
            ? `Due for review since ${dateLabel(review.due)}.`
            : `Reviewed every ${review.every_days} days · next ${dateLabel(review.due)}.`
          : 'No review date.'}
      </span>
      {item.editable && (
        <>
          <select
            aria-label="Review every"
            value={review?.every_days ?? ''}
            disabled={mutation.busy}
            onChange={(e) =>
              set({ every_days: e.target.value ? Number(e.target.value) : null, reviewed: false })
            }
          >
            <option value="">No review</option>
            {intervals.map((days) => (
              <option key={days} value={days}>
                Every {days} days
              </option>
            ))}
          </select>
          {review && (
            <Button
              disabled={mutation.busy}
              onClick={() => set({ every_days: null, reviewed: true })}
            >
              Mark reviewed
            </Button>
          )}
        </>
      )}
      {mutation.status && <span role="alert">{mutation.status}</span>}
    </div>
  );
}

type Gap = { id: string; query: string; asked: number; last_at: number; agent: string };
/** Questions agents searched the Library for and found nothing (LB5), for admins. */
export function UnansweredQuestions() {
  const load = useCallback(
    () => identityRequest<{ gaps: Gap[] }>(`/workspaces/${workspaceId}/library-gaps`),
    [],
  );
  const gaps = useResource(load);
  const mutation = useMutation();
  const writing = useRef(new Map<string, { id: string; request: string }>());
  const close = (gap: Gap, action: 'answered' | 'dismissed') =>
    identityRequest(`/workspaces/${workspaceId}/library-gaps/${gap.id}`, { action });
  const answer = (gap: Gap) =>
    void mutation.run(async () => {
      if (!writing.current.has(gap.id))
        writing.current.set(gap.id, { id: crypto.randomUUID(), request: crypto.randomUUID() });
      const page = writing.current.get(gap.id)!;
      await request('', {
        ...page,
        revision: 0,
        title: gap.query.slice(0, 180),
        document: emptyDocument,
      });
      await close(gap, 'answered');
      navigate(itemUrl(page.id));
    });
  return (
    <section className="library-gaps" aria-label="Unanswered questions">
      <p className="library-muted">
        Questions agents searched the Library for and found nothing. Who asked isn't shown. Write a
        page that answers one, or dismiss it.
      </p>
      {(gaps.error || mutation.status) && (
        <p className="library-notice" role="alert">
          {gaps.error || mutation.status}
        </p>
      )}
      {gaps.data && !gaps.data.gaps.length && (
        <p className="library-empty">No unanswered questions.</p>
      )}
      <ul>
        {gaps.data?.gaps.map((gap) => (
          <li key={gap.id}>
            <div>
              <strong>{gap.query}</strong>
              <small>
                Asked {gap.asked} time{gap.asked === 1 ? '' : 's'} · {gap.agent} ·{' '}
                {dateLabel(gap.last_at)}
              </small>
            </div>
            <Button primary disabled={mutation.busy} onClick={() => answer(gap)}>
              Write the answer
            </Button>
            <Button
              disabled={mutation.busy}
              onClick={() =>
                void mutation.run(async () => {
                  await close(gap, 'dismissed');
                  await gaps.refresh();
                })
              }
            >
              Dismiss
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}
