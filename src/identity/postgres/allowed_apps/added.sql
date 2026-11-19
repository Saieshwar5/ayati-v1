-- Schema 135: apps a workspace's admins added from a provider's library. Each
-- row is one app for one workspace: which library app it is and the actions the
-- admin reviewed, [{"id","title","effect","tool","fields"}]. It is offered,
-- allowed (relay_control_systems) and connected like a built-in app.
CREATE TABLE relay_added_apps (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 app TEXT NOT NULL CHECK(char_length(app) BETWEEN 1 AND 64),
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 description TEXT NOT NULL CHECK(char_length(description) BETWEEN 1 AND 80),
 category TEXT NOT NULL,
 provider TEXT NOT NULL,
 toolkit TEXT NOT NULL CHECK(char_length(toolkit) BETWEEN 1 AND 100),
 actions JSONB NOT NULL CHECK(jsonb_typeof(actions)='array' AND jsonb_array_length(actions) BETWEEN 1 AND 20),
 revision BIGINT NOT NULL DEFAULT 1,
 added_by TEXT NOT NULL,
 added_at BIGINT NOT NULL,
 changed_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,app)
);
