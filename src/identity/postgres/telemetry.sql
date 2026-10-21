-- Existing rows deliberately remain unmeasured; never invent historical latency.
ALTER TABLE relay_chat_jobs ADD COLUMN created_ms BIGINT, ADD COLUMN started_ms BIGINT, ADD COLUMN finished_ms BIGINT;
ALTER TABLE relay_chat_jobs ALTER COLUMN created_ms SET DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint;
CREATE INDEX relay_messages_pending ON relay_chat_messages(workspace) WHERE kind='agent' AND state='queued';
