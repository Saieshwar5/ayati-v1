-- Schema 80 (Apps BA3): agents an app's builder asked to work on its draft. An agent
-- writes the draft only in requests from that builder; publishing stays with people.
CREATE TABLE relay_built_app_helpers (
 workspace TEXT NOT NULL, app TEXT NOT NULL, agent TEXT NOT NULL,
 given_by TEXT NOT NULL REFERENCES relay_users(id), given_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,app,agent),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_built_app_helpers_agent ON relay_built_app_helpers(workspace,agent);
