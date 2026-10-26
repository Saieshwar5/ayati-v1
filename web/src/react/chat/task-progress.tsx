import { useState } from 'react';
import { storageScope, workspaceSession } from '../../workspace-context';
import { Button, Dialog } from '../ui';
import { useMutation } from '../requests';
import { requestLabels } from '../agents/request-model';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { QuestionDialog } from '../questions/dialog';
import { CoordinationView } from './handoffs';
import { chatRequest, type Message } from './model';
import { useTaskProgress } from './task-progress-state';
import './task-progress.css';
export function TaskProgress({ message }: { message: Message }) {
  const [open, setOpen] = useState(false);
  if (!message.chat) return null;
  return (
    <>
      {message.has_task && !message.response_unavailable && (
        <Button className="chat-check-progress" onClick={() => setOpen(true)}>
          Check progress
        </Button>
      )}
      {open && (
        <TaskProgressDialog
          key={storageScope(`${message.chat}.${message.id}`)}
          chat={message.chat}
          reply={message.id}
          close={() => setOpen(false)}
        />
      )}
    </>
  );
}
export function TaskProgressDialog({
  chat,
  reply,
  close,
}: {
  chat: string;
  reply: string;
  close: () => void;
}) {
  const { data, error, busy, refresh } = useTaskProgress(chat, reply);
  const mutation = useMutation();
  const [question, setQuestion] = useState<string>();
  const [approval, setApproval] = useState<string>();
  const state = data && (requestLabels[data.state] ?? 'Status unavailable');
  const time = (value: number) => (
    <time dateTime={new Date(value).toISOString()}>{new Date(value).toLocaleString()}</time>
  );
  const pendingQuestions = data?.questions.filter((q) => q.state === 'pending') ?? [];
  const pendingApprovals =
    data?.approvals.filter((a) => ['pending', 'executing', 'uncertain'].includes(a.state)) ?? [];
  return (
    <Dialog title="Task progress" drawer close={close}>
      <div className="task-progress">
        <p className="ui-help">Saved controller status{data && <> · Checked {time(data.checked_ms)}</>}</p>
        {error && (
          <p role="alert">
            {error}
            {data && ' Showing the last successful check.'}
          </p>
        )}
        {!data && !error && <p role="status">Checking saved progress…</p>}
        {!data && error && <p>Task progress is unavailable. Current access is required to view this work.</p>}
        {data && (
          <>
            <section aria-label="Current task state">
              <h3 aria-live="polite">{state}</h3>
              {data.worker_released && <p className="ui-help">Worker released while waiting.</p>}
              {data.state === 'recovering' && (
                <p className="ui-help">
                  The last worker lease expired. A replacement has not yet been confirmed.
                </p>
              )}
              {data.state === 'attention' && (
                <p className="ui-help">
                  An external outcome is not confirmed. Checking progress does not retry it.
                </p>
              )}
              <h4>{data.summary}</h4>
              <p className="ui-help">
                Requested by {data.requester_name ?? 'Former member'} · {data.conversation_name}
                {data.thread && ' · Thread'}
              </p>
            </section>
            {data.latest_update && (
              <section aria-label="Latest saved update">
                <h4>Latest saved update</h4>
                <p className="task-progress-update">{data.latest_update}</p>
              </section>
            )}
            {!!pendingQuestions.length && (
              <section aria-label="Needs input">
                <h4>Needs input</h4>
                {pendingQuestions.map((q) => (
                  <div className="task-progress-reference" key={q.id}>
                    <span>
                      {q.title}
                      <small>For {q.respondent_name}</small>
                    </span>
                    <Button onClick={() => setQuestion(q.id)}>
                      {q.respondent === workspaceSession?.user?.id ? 'Answer question' : 'View question'}
                    </Button>
                  </div>
                ))}
              </section>
            )}
            {!!pendingApprovals.length && (
              <section aria-label="Needs approval">
                <h4>Needs approval</h4>
                {pendingApprovals.map((a) => (
                  <div className="task-progress-reference" key={a.id}>
                    <span>
                      {a.title}
                      <small>
                        {a.state === 'pending'
                          ? 'Waiting for approval'
                          : a.state === 'uncertain'
                            ? 'Outcome not confirmed'
                            : 'Executing approved action'}
                      </small>
                    </span>
                    <Button onClick={() => setApproval(a.id)}>Review {a.title}</Button>
                  </div>
                ))}
              </section>
            )}
            <CoordinationView data={data.coordination} />
            <dl className="task-progress-times">
              <div>
                <dt>Accepted</dt>
                <dd>{time(data.created_ms)}</dd>
              </div>
              {data.started_ms && (
                <div>
                  <dt>Started</dt>
                  <dd>{time(data.started_ms)}</dd>
                </div>
              )}
              {data.finished_ms && (
                <div>
                  <dt>Finished</dt>
                  <dd>{time(data.finished_ms)}</dd>
                </div>
              )}
            </dl>
            {data.cancellation && (
              <p className="ui-help">
                Cancellation recorded by {data.cancellation.by}. Dispatched effects retain their recorded
                outcomes.
              </p>
            )}
          </>
        )}
        <div className="task-progress-actions">
          <Button disabled={busy} onClick={refresh}>
            {busy ? 'Checking…' : error && !data ? 'Retry' : 'Check now'}
          </Button>
          {data?.can_cancel && (
            <Button
              disabled={mutation.busy}
              onClick={() =>
                void mutation.run(async () => {
                  await chatRequest(`/chats/${chat}/messages/${reply}/cancel`, {});
                  refresh();
                })
              }
            >
              {mutation.busy ? 'Saving cancellation…' : 'Stop request'}
            </Button>
          )}
        </div>
        {mutation.status && <p role="alert">{mutation.status}</p>}
        <p className="ui-help">Checking progress reads saved records. It starts no new agent work.</p>
      </div>
      {data && question && (
        <QuestionDialog
          key={question}
          id={question}
          close={() => {
            setQuestion(undefined);
            refresh();
          }}
        />
      )}
      {data && approval && (
        <ApprovalDialog
          key={approval}
          id={approval}
          close={() => {
            setApproval(undefined);
            refresh();
          }}
        />
      )}
    </Dialog>
  );
}
