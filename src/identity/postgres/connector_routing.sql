-- Schema 108 (connector layer P2): which provider serves each catalog app per
-- workspace, the account identity a connection proved, who connected it and
-- when, and one-step connect requests.

-- Cuckoo's administrators route one workspace's app to a provider. Without a
-- row the providers file's route applies, then the catalog default. Only new
-- connections follow a route; a saved connection keeps its provider.
CREATE TABLE relay_connector_routes (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 app TEXT NOT NULL CHECK(app ~ '^[A-Za-z0-9_-]{1,128}$'),
 provider TEXT NOT NULL CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 note TEXT NOT NULL DEFAULT '' CHECK(length(note)<=300),
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed BIGINT NOT NULL,
 PRIMARY KEY(workspace,app)
);

-- `identity` is a digest of the app's own account identity (a mailbox) when the
-- provider proved it: reconnecting the same identity with an unchanged reviewed
-- definition keeps the account's rules. NULL always starts over.
ALTER TABLE relay_control_connections
 ADD COLUMN identity TEXT CHECK(identity IS NULL OR length(identity) BETWEEN 16 AND 128),
 ADD COLUMN connected_by TEXT REFERENCES relay_users(id),
 ADD COLUMN connected_at BIGINT;
-- Existing connections: who last saved them and when, from the audit log.
-- Changing a connection row fences running work as a change of authority; this
-- records history only, so the triggers are off for this one statement.
ALTER TABLE relay_control_connections DISABLE TRIGGER USER;
UPDATE relay_control_connections c SET connected_by=a.actor,connected_at=a.at
FROM (SELECT DISTINCT ON (workspace,target) workspace,target,actor,at FROM relay_audit
      WHERE action IN ('workspace_connection_saved','control_connected')
      ORDER BY workspace,target,id DESC) a
WHERE a.workspace=c.workspace AND a.target=c.id AND c.enabled
 AND EXISTS(SELECT 1 FROM relay_users u WHERE u.id=a.actor);
ALTER TABLE relay_control_connections ENABLE TRIGGER USER;

-- One-step connect from Add app: the same request id returns the same answer.
-- `answer` is NULL while the connect is in progress; a failed one is removed.
CREATE TABLE relay_connector_connects (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 request TEXT NOT NULL,
 app TEXT NOT NULL,
 connection TEXT NOT NULL,
 requested_by TEXT NOT NULL REFERENCES relay_users(id),
 answer JSONB CHECK(answer IS NULL OR octet_length(answer::text)<=4000),
 created BIGINT NOT NULL,
 PRIMARY KEY(workspace,request)
);

-- A member's request for a catalog app ("Request" in Add app) is a suggestion
-- that remembers the app, so the catalog can show it as requested.
ALTER TABLE relay_app_requests
 ADD COLUMN connector TEXT CHECK(connector IS NULL OR (kind='suggestion' AND connector ~ '^[A-Za-z0-9_-]{1,128}$'));
CREATE UNIQUE INDEX relay_app_requests_one_connector ON relay_app_requests(workspace,connector,requester)
 WHERE state='pending' AND connector IS NOT NULL;

-- Writes on a connection wait while an earlier write's outcome is unconfirmed.
CREATE INDEX relay_control_actions_open_writes ON relay_control_actions(workspace,connection)
 WHERE effect='write' AND state IN ('executing','uncertain');
CREATE FUNCTION relay_paused_write(w TEXT, c TEXT) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT a.id FROM relay_control_actions a
 WHERE a.workspace=w AND a.connection=c AND a.effect='write'
  AND (a.state='uncertain' OR (a.state='executing' AND a.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60))
 ORDER BY a.created,a.id LIMIT 1 $$;
-- A connection's health as screens show it: paused (Cuckoo switched its provider
-- off), reconnect (its sign-in stopped working), writes_paused, or ok.
CREATE FUNCTION relay_connection_state(w TEXT, c TEXT, p TEXT) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT CASE
  WHEN EXISTS(SELECT 1 FROM relay_connector_providers x WHERE x.provider=p AND NOT x.enabled) THEN 'paused'
  WHEN EXISTS(SELECT 1 FROM relay_connector_reconnects n WHERE n.workspace=w AND n.connection=c) THEN 'reconnect'
  WHEN relay_paused_write(w,c) IS NOT NULL THEN 'writes_paused'
  ELSE 'ok' END $$;
