-- Data lifecycle (D226). History is kept; only duplicates and delivery notices
-- are removed, and the dispatcher's indexes cover active work only.

-- The partial text of a stopped reply, saved before its streamed chunks (a
-- duplicate of message text) are removed. A separate table: saving it is not a
-- message edit, so it fires no revision, live-update or context triggers.
CREATE TABLE relay_reply_partials (
 workspace TEXT NOT NULL, reply TEXT NOT NULL,
 text TEXT NOT NULL CHECK(octet_length(text)<=65536),
 PRIMARY KEY(workspace,reply),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id)
);
CREATE INDEX relay_assignments_finished ON relay_worker_assignments(finished_ms)
 WHERE state IN ('finished','lost');

-- Live-update notices age out; a browser asking from before a workspace's
-- floor is told to reload instead of silently missing changes.
ALTER TABLE relay_sync_events ADD COLUMN created_ms BIGINT NOT NULL
 DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint;
CREATE INDEX relay_sync_events_age ON relay_sync_events(created_ms);
ALTER TABLE relay_sync_clocks ADD COLUMN floor BIGINT NOT NULL DEFAULT 0;

-- The dispatcher searches queued and running work only, however much finished
-- history accumulates.
DROP INDEX relay_jobs_dispatch_ready;
CREATE INDEX relay_jobs_dispatch_ready ON relay_chat_jobs(dispatch,state,lease_until,sequence)
 WHERE state<>'finished';
DROP INDEX relay_jobs_ready;
CREATE INDEX relay_jobs_ready ON relay_chat_jobs(state,lease_until) WHERE state<>'finished';
