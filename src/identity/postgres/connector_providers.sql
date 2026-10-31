-- Connector providers (schema 53). A connection records which provider serves
-- it and the provider's own identity for the account (never a token).
ALTER TABLE relay_control_connections
 ADD COLUMN provider TEXT NOT NULL DEFAULT 'mcp' CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 ADD COLUMN binding JSONB CHECK(binding IS NULL OR octet_length(binding::text)<=2000);

-- Accounts whose sign-in expired or was revoked at the provider. Kept apart
-- from the connection row on purpose: changing that row fences running work as
-- a change of authority, and an expired sign-in is not one.
CREATE TABLE relay_connector_reconnects (
 workspace TEXT NOT NULL, connection TEXT NOT NULL, since BIGINT NOT NULL,
 PRIMARY KEY(workspace,connection),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id) ON DELETE CASCADE
);

-- A sign-in in progress: the provider's page is open in the admin's browser.
CREATE TABLE relay_connector_sign_ins (
 id TEXT PRIMARY KEY,
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 connection TEXT NOT NULL, server TEXT NOT NULL, provider TEXT NOT NULL,
 binding JSONB NOT NULL CHECK(octet_length(binding::text)<=2000),
 revision BIGINT NOT NULL,
 started_by TEXT NOT NULL REFERENCES relay_users(id),
 state TEXT NOT NULL DEFAULT 'waiting' CHECK(state IN ('waiting','connected','failed')),
 created BIGINT NOT NULL, expires BIGINT NOT NULL
);
CREATE INDEX relay_connector_sign_ins_workspace ON relay_connector_sign_ins(workspace,created);

-- Cuckoo's own switch per provider: when one has an incident, every call
-- through it stops at once and every other provider keeps working.
CREATE TABLE relay_connector_providers (
 provider TEXT PRIMARY KEY CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 enabled BOOLEAN NOT NULL,
 note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=300),
 changed_by TEXT REFERENCES relay_users(id),
 changed BIGINT NOT NULL
);
