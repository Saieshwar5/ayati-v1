-- Your own computer (schema 119, B3 of agent building blocks, D248). A
-- workspace adds a machine it runs itself; a small program on it (the Cuckoo
-- Connector) signs in once with a one-time code and then connects outward to
-- take work, so no ports are opened. Admins choose which agents use it. Each
-- agent uses exactly one computer: none, Cuckoo's, or one of these.
CREATE TABLE relay_workspace_computers (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 100),
 -- The one-time code (digest) and when it stops working; cleared once used.
 code TEXT, code_expires_ms BIGINT,
 -- The connector's lasting key (digest), set when it signs in.
 device TEXT, platform TEXT CHECK(platform IS NULL OR length(platform)<=100),
 last_seen_ms BIGINT,
 -- One request at a time, reusing the waiting line's rule of holding.
 held_by TEXT, held_until BIGINT NOT NULL DEFAULT 0,
 removed BOOLEAN NOT NULL DEFAULT FALSE,
 revision BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id)
);
CREATE TABLE relay_workspace_computer_agents (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, computer TEXT NOT NULL,
 chosen_by TEXT NOT NULL REFERENCES relay_users(id),
 chosen_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,agent),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,computer) REFERENCES relay_workspace_computers(workspace,id)
);
-- Work for a connector: a command, or a file to read or write, in one agent's
-- folder. The controller waits for the answer; nothing is ever retried.
CREATE TABLE relay_workspace_computer_calls (
 id TEXT PRIMARY KEY, workspace TEXT NOT NULL, computer TEXT NOT NULL, agent TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('run','write','read','list')),
 request JSONB NOT NULL CHECK(octet_length(request::text) <= 1500000),
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','taken','done','abandoned')),
 result JSONB CHECK(result IS NULL OR octet_length(result::text) <= 1500000),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 taken_ms BIGINT, done_ms BIGINT,
 FOREIGN KEY(workspace,computer) REFERENCES relay_workspace_computers(workspace,id)
);
CREATE INDEX relay_workspace_computer_calls_queued ON relay_workspace_computer_calls(workspace,computer,created_ms) WHERE state='queued';
