-- Settings completed (D232 slices 2–5, D233 slice 5). Personal notification
-- defaults, the workspace's time zone, colour, models and scheduled deletion,
-- and problem reports for the Cuckoo team.

-- Each person's defaults; channels without their own bell level use channel_level.
CREATE TABLE relay_member_preferences (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 desktop_alerts BOOLEAN NOT NULL DEFAULT TRUE,
 channel_level TEXT NOT NULL DEFAULT 'mentions' CHECK(channel_level IN ('all','mentions','none')),
 approval_alerts BOOLEAN NOT NULL DEFAULT TRUE,
 PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);

-- models: the provider/model pairs agents may use; an empty list allows any.
-- timezone: unset until an admin chooses one; Home then shows device time.
-- delete_at_ms: a scheduled deletion that admins can undo until then.
-- closed_ms: set when the deletion completes; nobody can open the workspace.
ALTER TABLE relay_workspaces
 ADD COLUMN timezone TEXT CHECK(length(timezone) BETWEEN 1 AND 64),
 ADD COLUMN colour TEXT CHECK(colour IN ('blue','green','orange','purple','brown','teal')),
 ADD COLUMN models JSONB NOT NULL DEFAULT '[]'::jsonb
  CHECK(jsonb_typeof(models)='array' AND jsonb_array_length(models)<=20),
 ADD COLUMN default_model TEXT CHECK(length(default_model) BETWEEN 1 AND 200),
 ADD COLUMN delete_at_ms BIGINT,
 ADD COLUMN delete_requested_by TEXT,
 ADD COLUMN closed_ms BIGINT;
CREATE INDEX relay_workspaces_deleting ON relay_workspaces(delete_at_ms) WHERE delete_at_ms IS NOT NULL AND closed_ms IS NULL;

-- "Report a problem" goes to the Cuckoo team, not to the workspace. Reports keep
-- the workspace id even after it is deleted.
CREATE TABLE relay_problem_reports (
 id TEXT PRIMARY KEY CHECK(length(id) BETWEEN 1 AND 100),
 workspace TEXT NOT NULL,
 reporter TEXT NOT NULL,
 topic TEXT NOT NULL CHECK(topic IN ('agent','chat','connections','library','billing','other')),
 body TEXT NOT NULL CHECK(length(body) BETWEEN 1 AND 4000),
 technical JSONB,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint
);
CREATE INDEX relay_problem_reports_recent ON relay_problem_reports(created_ms DESC);
