import { useCallback, useEffect, useRef } from 'react';
import { identityRequest, workspaceId } from '../../workspace-context';
import { useMutation, useResource } from '../requests';
import { Button, Dialog } from '../ui';
import { subscribeChanges } from '../chat/live';
import { QuestionForm } from './form';
import { answerLabels, questionState, type Question } from './model';

export function QuestionDialog({ id, close }: { id: string; close: () => void }) {
  const path = `/workspaces/${workspaceId}/questions/${encodeURIComponent(id)}`;
  return <QuestionReview key={path} path={path} close={close} />;
}
function QuestionReview({ path, close }: { path: string; close: () => void }) {
  const load = useCallback(() => identityRequest<Question>(path), [path]);
  const { data, error, refresh, setData } = useResource(load);
  const mutation = useMutation();
  const current = useRef(path), alive = useRef(false);
  current.current = path;
  const attempt = useRef<{ payload: string; id: string }>(undefined);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  useEffect(() => {
    const timer = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, 5000);
    const unsubscribe = subscribeChanges(change => {
      if ((change.kind === 'access' || change.kind === 'reset') && (!change.chat || !data || change.chat === data.chat)) {
        setData(undefined); void refresh().catch(() => {});
      } else if (change.kind === 'message' && change.message?.id === data?.reply) void refresh().catch(() => {});
    });
    return () => { window.clearInterval(timer); unsubscribe(); };
  }, [refresh, setData, data?.chat, data?.reply]);
  const respond = (decision: 'answer' | 'decline', values: Record<string, unknown>) => void mutation.run(async () => {
    if (!data?.can_respond) throw new Error('This question is no longer available to answer.');
    const payload = JSON.stringify({ decision, values, revision: data.revision, path });
    if (attempt.current?.payload !== payload) attempt.current = { payload, id: crypto.randomUUID() };
    const saved = await identityRequest<Question>(path, { id: attempt.current.id, revision: data.revision, decision, values });
    if (alive.current && current.current === path) setData({ ...data, ...saved, can_respond: false });
  });
  return <Dialog title={data?.specification.title ?? 'Agent question'} drawer busy={mutation.busy} status={mutation.status} close={close}>
    <Button disabled={mutation.busy} onClick={() => void refresh().catch(() => {})}>Refresh question</Button>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading question…</p>}
    {data && <div className="question-review"><p className="question-state" role="status">{questionState(data.state)}</p>
      <p className="ui-help">For {data.respondent_name}. An answer supplies facts; business actions retain their own permissions.</p>
      {data.specification.description && <p className="question-description">{data.specification.description}</p>}
      <QuestionContent key={`${path}:${data.revision}`} question={data} busy={mutation.busy} respond={respond} />
    </div>}
  </Dialog>;
}

function QuestionContent({ question, busy, respond }: {
  question: Question;
  busy: boolean;
  respond: (decision: 'answer' | 'decline', values: Record<string, unknown>) => void;
}) {
  const specification = question.specification;
  if (specification.version !== 1 || !['single_choice', 'multiple_choice', 'form'].includes(specification.kind)) {
    return <p>{specification.fallback || 'This question needs a newer supported client. No answer has been submitted here.'}</p>;
  }
  if (question.can_respond && question.state === 'pending') {
    return <QuestionForm question={question} busy={busy} respond={respond} />;
  }
  if (question.state === 'answered') {
    return <>
      <dl className="question-answer">
        {answerLabels(question).map(answer => <div key={answer.id}>
          <dt>{answer.label}</dt><dd>{answer.value}</dd>
        </div>)}
      </dl>
      {question.answered_ms && <p className="ui-help">
        Saved {new Date(question.answered_ms).toLocaleString()}. This saved answer cannot rewrite work already dispatched.
      </p>}
    </>;
  }
  if (question.state === 'pending') {
    return <p>Only the intended requester can answer. You can continue the conversation without submitting on their behalf.</p>;
  }
  return <p>{question.state === 'declined'
    ? 'The question was declined without supplying an answer.'
    : 'This question is no longer active. No new answer can be submitted.'}</p>;
}
