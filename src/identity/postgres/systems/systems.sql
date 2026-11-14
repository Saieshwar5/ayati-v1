-- Connected systems (schema 116, C1 of the connected systems plan, D249). A
-- company's own MCP server, connected by an agent's owner or an admin from the
-- agent's Tools tab. Its connection is an ordinary relay_control_connections
-- row with provider 'system' (address and sealed key in its binding), so every
-- call passes the usual gateway, rules, approvals and records. This row keeps
-- what the person reviewed and who did it.
CREATE TABLE relay_connected_systems (
 workspace TEXT NOT NULL, connection TEXT NOT NULL, agent TEXT NOT NULL,
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 100),
 -- The tools as last reviewed: [{name,title,kind,rule}]; switched-on tools only.
 tools JSONB NOT NULL DEFAULT '[]'::jsonb,
 request_hash TEXT NOT NULL,
 removed BOOLEAN NOT NULL DEFAULT FALSE,
 revision BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 reviewed_by TEXT REFERENCES relay_users(id), reviewed_ms BIGINT,
 PRIMARY KEY(workspace,connection),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id)
);
CREATE INDEX relay_connected_systems_agent ON relay_connected_systems(workspace,agent) WHERE NOT removed;
