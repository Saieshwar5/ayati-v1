-- Tasks (people-and-agents-first, Phase 2): a task is a promise with an outcome.
-- It groups the requests that work towards one result. Its status is derived from
-- those requests, approvals and questions; only its closing outcome is stored.
--   kind automatic:  the platform opened it because a request changed a real
--                    system, waited for someone, or came from a schedule or event.
--                    It closes with that request.
--   kind commitment: an agent promised it (create_task) or a person asked for it
--                    (Make this a task). It stays open until closed.
CREATE TABLE relay_tasks (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 agent TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('automatic','commitment')),
 origin TEXT NOT NULL CHECK(origin IN ('request','schedule','event','agent','person')),
 title TEXT NOT NULL CHECK(char_length(title) BETWEEN 1 AND 160),
 done_when TEXT CHECK(done_when IS NULL OR char_length(done_when) BETWEEN 1 AND 400),
 requester TEXT NOT NULL REFERENCES relay_users(id), chat TEXT NOT NULL,
 created_by TEXT REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 start_ms BIGINT, due_ms BIGINT,
 outcome TEXT CHECK(outcome IN ('done','failed','cancelled','not_task')),
 result TEXT CHECK(result IS NULL OR char_length(result) BETWEEN 1 AND 1000),
 closed_by TEXT REFERENCES relay_users(id), closed_ms BIGINT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 CHECK((outcome IS NULL)=(closed_ms IS NULL))
);
CREATE INDEX relay_tasks_open ON relay_tasks(workspace,agent,created_ms DESC) WHERE outcome IS NULL;
CREATE INDEX relay_tasks_requester ON relay_tasks(workspace,requester,created_ms DESC);
ALTER TABLE relay_chat_jobs ADD COLUMN task TEXT;
ALTER TABLE relay_chat_jobs ADD CONSTRAINT relay_chat_jobs_task
 FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id);
CREATE INDEX relay_chat_jobs_task ON relay_chat_jobs(workspace,task,sequence) WHERE task IS NOT NULL;

-- Follow-ups the agent booked for a task (create_task with start_in_minutes, or
-- follow_up): when due, the agent continues the task in its conversation, for the
-- person it promised. Blocked when that person can no longer ask the agent there.
CREATE TABLE relay_task_wakeups (
 workspace TEXT NOT NULL, id TEXT NOT NULL, task TEXT NOT NULL,
 due_ms BIGINT NOT NULL CHECK(due_ms>0),
 note TEXT CHECK(note IS NULL OR char_length(note) BETWEEN 1 AND 400),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','started','cancelled','blocked')),
 reply TEXT, message TEXT, detail TEXT NOT NULL DEFAULT '' CHECK(char_length(detail)<=300),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id)
);
CREATE INDEX relay_task_wakeups_due ON relay_task_wakeups(due_ms) WHERE state='pending';
CREATE INDEX relay_task_wakeups_task ON relay_task_wakeups(workspace,task);

-- A task's status, worked out from its requests; never typed by hand.
--   done/failed/cancelled/not_task: closed (an automatic task closes with its
--     last request: complete is done, cancelled is cancelled, anything else failed);
--   waiting: a request waits for an approval, a question, a helper or a time;
--   working: a request is queued or running;
--   needs_fixing: a commitment whose latest request could not complete, or whose
--     follow-up could not start;
--   scheduled: a commitment that starts later or has a follow-up booked;
--   late: past its due time;
--   open: a commitment with nothing running, not yet due.
CREATE FUNCTION relay_task_status(w TEXT, task_id TEXT) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT CASE
  WHEN t.outcome IS NOT NULL THEN t.outcome
  WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=w AND j.task=task_id AND j.state='waiting') THEN 'waiting'
  WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=w AND j.task=task_id AND j.state<>'finished') THEN 'working'
  WHEN t.kind='automatic' THEN COALESCE((SELECT CASE m.state WHEN 'complete' THEN 'done' WHEN 'cancelled' THEN 'cancelled' ELSE 'failed' END
   FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
   WHERE j.workspace=w AND j.task=task_id ORDER BY j.sequence DESC LIMIT 1),'done')
  WHEN (SELECT m.state FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
   WHERE j.workspace=w AND j.task=task_id ORDER BY j.sequence DESC LIMIT 1) IN ('failed','interrupted','access_changed') THEN 'needs_fixing'
  WHEN EXISTS(SELECT 1 FROM relay_task_wakeups k WHERE k.workspace=w AND k.task=task_id AND k.state='blocked'
   AND NOT EXISTS(SELECT 1 FROM relay_task_wakeups l WHERE l.workspace=w AND l.task=task_id AND l.created_ms>k.created_ms)) THEN 'needs_fixing'
  WHEN t.due_ms<(extract(epoch FROM clock_timestamp())*1000)::bigint THEN 'late'
  WHEN t.start_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint
   OR EXISTS(SELECT 1 FROM relay_task_wakeups k WHERE k.workspace=w AND k.task=task_id AND k.state='pending') THEN 'scheduled'
  ELSE 'open' END
 FROM relay_tasks t WHERE t.workspace=w AND t.id=task_id
$$;
