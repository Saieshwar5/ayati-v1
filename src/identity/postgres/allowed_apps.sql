-- Schema 134: the workspace's allowed apps. An admin's yes or no to an app can
-- carry a note and records who decided; a member's request names the agent it
-- is for and keeps the admin's answer. An MCP server a member connects waits
-- for an admin's yes before the agent can use it.
ALTER TABLE relay_control_systems
 ADD COLUMN note TEXT NOT NULL DEFAULT '' CHECK(char_length(note)<=300),
 ADD COLUMN decided_by TEXT,
 ADD COLUMN decided_at BIGINT;
ALTER TABLE relay_app_requests
 ADD COLUMN agent TEXT,
 ADD COLUMN note TEXT NOT NULL DEFAULT '' CHECK(char_length(note)<=300);
ALTER TABLE relay_connected_systems
 ADD COLUMN waiting BOOLEAN NOT NULL DEFAULT false;
