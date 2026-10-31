-- Activity (D235): one place for all agent work. It reads existing records; this
-- migration only adds each person's last visit and the indexes its filters need.
CREATE TABLE relay_activity_visits (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 seen_ms BIGINT NOT NULL,
 PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
-- Requests from before telemetry.sql have no creation time. Their reply's time is
-- the closest record, so Activity filters every request on one indexed column.
-- Queue metrics still skip them: they have no start or finish time.
UPDATE relay_chat_jobs j SET created_ms=m.at*1000 FROM relay_chat_messages m
 WHERE j.created_ms IS NULL AND m.workspace=j.workspace AND m.id=j.reply;
ALTER TABLE relay_chat_jobs ALTER COLUMN created_ms SET NOT NULL;
-- The feed walks newest first; totals and agent cards read a date range.
CREATE INDEX relay_jobs_activity ON relay_chat_jobs(workspace,sequence DESC);
CREATE INDEX relay_jobs_activity_time ON relay_chat_jobs(workspace,created_ms);
CREATE INDEX relay_jobs_activity_agent ON relay_chat_jobs(workspace,(payload->>'agent'),created_ms);
CREATE INDEX relay_jobs_activity_requester ON relay_chat_jobs(workspace,(payload->>'requester'),created_ms);
CREATE INDEX relay_control_actions_job ON relay_control_actions(workspace,job);
CREATE INDEX relay_audit_agent_changes ON relay_audit(workspace,action,id DESC);
