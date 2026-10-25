CREATE TABLE relay_control_connections (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 server TEXT NOT NULL, definition JSONB NOT NULL, fingerprint TEXT NOT NULL,
 enabled BOOLEAN NOT NULL DEFAULT TRUE, revision BIGINT NOT NULL DEFAULT 1,
 PRIMARY KEY(workspace,id), UNIQUE(workspace,server)
);
CREATE TABLE relay_control_rules (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, connection TEXT NOT NULL,
 operation TEXT NOT NULL, mode TEXT NOT NULL CHECK(mode IN ('allow','ask','block')),
 approver TEXT NOT NULL REFERENCES relay_users(id), revision BIGINT NOT NULL,
 chat TEXT NOT NULL,
 PRIMARY KEY(workspace,agent,connection,operation),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_control_actions (
 id TEXT PRIMARY KEY, workspace TEXT NOT NULL, agent TEXT NOT NULL, chat TEXT NOT NULL,
 thread TEXT, job TEXT NOT NULL, requester TEXT NOT NULL REFERENCES relay_users(id),
 request_id TEXT NOT NULL, connection TEXT NOT NULL, operation TEXT NOT NULL,
 arguments JSONB NOT NULL, title TEXT NOT NULL, effect TEXT NOT NULL,
 rule_revision BIGINT NOT NULL, connection_revision BIGINT NOT NULL,
 approver TEXT NOT NULL REFERENCES relay_users(id),
 state TEXT NOT NULL CHECK(state IN ('pending','executing','completed','denied','rejected','invalidated','uncertain')),
 result JSONB NOT NULL DEFAULT '{}'::jsonb, decision_id TEXT, decision TEXT,
 dispatch_token TEXT, created BIGINT NOT NULL, updated BIGINT NOT NULL,
 UNIQUE(workspace,job,request_id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id)
);
CREATE INDEX relay_control_actions_agent ON relay_control_actions(workspace,agent,created DESC);
