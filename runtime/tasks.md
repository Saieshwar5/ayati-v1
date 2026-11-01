# Tasks (schema 58)

People-and-agents-first, Phase 2 (planned in `proj-docs/people-and-agents-first/tasks.md`).
**A task is a promise with an outcome.** It groups the requests that work towards one
result, so people see what agents have taken on, and an agent never forgets a promise.

## Two kinds

| Kind | Opened by | Closes |
| --- | --- | --- |
| **automatic** | The platform, when a request changes a real system (a write action that isn't blocked), asks a question, hands work to a helper, or comes from a schedule or an event | With that request: complete is done, cancelled is cancelled, anything else failed |
| **commitment** | The agent's `create_task`, or a person's **Make this a task** | Only when the agent calls `close_task` or a person closes it |

A Home panel refresh is a panel update, never a task. Answering, explaining, drafting text
for the person or giving an opinion is conversation: no task.

## Status

`relay_task_status(workspace, task)` works the status out from the task's requests; only
the closing outcome is stored.

| Status | Means |
| --- | --- |
| working | A request is queued or running |
| waiting | A request waits for an approval, an answer, a helper or a time (`waiting_on` names which) |
| needs_fixing | A commitment whose latest request could not complete |
| scheduled / late | A commitment that starts later / is past its due time |
| open | A commitment with nothing running, not yet due |
| done / failed / cancelled / not_task | Closed |

## Agent tools (ayati_control)

- `open_tasks {}`: this agent's open commitments for this person or conversation.
- `create_task {title, done_when?}`: title up to 160 characters, done_when up to 400. If the
  request already has an automatic task, that task becomes the commitment (one task, not
  two). A different promise in the same request is refused.
- `attach_to_task {task}`: this request continues an open task of this agent, for the same
  person or conversation. An automatic task the request had is dropped.
- `close_task {task, outcome: done|failed|cancelled, result?}`: result up to 1000 characters.
  Repeating the same outcome is harmless.

The agent's instructions carry the promise rule: never say "I'll…" without a task; check
`open_tasks` and attach instead of promising twice. A panel update task can't use these
tools.

## People

- **Activity → Tasks**: every open task the person may see, needs fixing first, then late,
  waiting, working, open and scheduled. My agents narrows it. At most 200 are listed.
- **Task details**: what it promises (done when), who it's for, where, its steps (each
  request, opening its own details) and its result. **Mark done** and **Cancel task** are
  for the person it's for, the agent's managers and admins; **Not a task** is for anyone
  who can see it. Each close is in the audit log.
- **Make this a task** on a request's details turns it into a commitment for the person who
  asked, kept open until someone closes it.
- **Chat** shows a chip under an agent reply: "Task: Send quotes to Acme · Open →" for a
  promise; work the platform tracked shows ("Tracked: …") only once it waits, needs a look
  or is done. History shows a request's task, and conversation turns that aren't part of
  any task read lighter.

**Who sees a task:** the person it is for, anyone who can see its conversation, the agent's
managers and admins.

## API

Under `/api/identity/workspaces/{w}/activity`:

- `GET /tasks?scope=mine|all` — open tasks.
- `GET /tasks/{id}` — one task with its requests and `may_close`.
- `POST /tasks/{id}/close {outcome: done|cancelled|not_task, result?}`.
- `POST /requests/{reply}/task {title?}` — Make this a task.

History items and request details carry `task {id, title, kind, status}`; Chat messages
carry the same `task` for agent replies.

## Storage

`relay_tasks` (kind, origin, title, done_when, requester, chat, start/due times, outcome,
result, who closed it) and `relay_chat_jobs.task`. Upgrade tests run current code on older
schemas, where requests are simply not tracked (`tasks::available`).

## Checks

- PostgreSQL `tasks::tests`: a write makes a task that closes with its request and shows in
  Chat; an agent's promise stays open, takes more work and closes; a question waits and a
  failed promise needs fixing (listed first); people make and close tasks and only those
  who may see one do.
- Guest `task_schema`: the tools forward only their bounded fields.
- Web `tests/activity-tasks.test.mjs`.
- Journey `tests/agent_tasks.mjs` (real Goose, scripted model): promise, attach, close.
