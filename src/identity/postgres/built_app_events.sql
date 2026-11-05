-- Schema 84 (Apps AP10): app events become agent work. The app's builder or an admin
-- asks an agent to act when a record is added to a collection; each new live record is
-- queued once and handled in that person's conversation with the agent, as them.
CREATE TABLE relay_built_app_watchers (
 workspace TEXT NOT NULL, app TEXT NOT NULL, collection TEXT NOT NULL,
 agent TEXT NOT NULL, principal TEXT NOT NULL REFERENCES relay_users(id), chat TEXT NOT NULL,
 instructions TEXT NOT NULL CHECK(char_length(instructions) BETWEEN 1 AND 2000),
 set_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,app,collection),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_built_app_events (
 workspace TEXT NOT NULL, app TEXT NOT NULL, collection TEXT NOT NULL, record TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','started','skipped')),
 reply TEXT, detail TEXT NOT NULL DEFAULT '', at BIGINT NOT NULL,
 PRIMARY KEY(workspace,app,collection,record),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_built_app_events_waiting ON relay_built_app_events(at) WHERE state='pending';
