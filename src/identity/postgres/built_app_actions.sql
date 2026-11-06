-- Schema 81 (Apps AP10): what agents may do in built apps. The builder or an admin
-- allows each action the manifest offers; until then agents can't run it. A run is
-- remembered by a one-time key, so a retried request never repeats a change, and a
-- record says which agent made it for the person.
CREATE TABLE relay_built_app_agent_rules (
 workspace TEXT NOT NULL, app TEXT NOT NULL,
 action TEXT NOT NULL CHECK(char_length(action) BETWEEN 1 AND 40),
 mode TEXT NOT NULL CHECK(mode IN ('allow','block')),
 set_by TEXT NOT NULL REFERENCES relay_users(id), set_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,app,action),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
CREATE TABLE relay_built_app_action_runs (
 workspace TEXT NOT NULL, key TEXT NOT NULL, app TEXT NOT NULL, action TEXT NOT NULL,
 agent TEXT NOT NULL, requester TEXT NOT NULL, result JSONB NOT NULL, at BIGINT NOT NULL,
 PRIMARY KEY(workspace,key),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
ALTER TABLE relay_built_app_records ADD COLUMN by_agent TEXT;
