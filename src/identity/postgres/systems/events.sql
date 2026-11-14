-- Events from connected systems (schema 117, C2 and C5 of the connected
-- systems plan, D249). A system sends events with its own key; each event type
-- an automation listens to is an event source in relay_connector_subscriptions
-- with provider 'system', so receipts, deliveries and dispatch are the usual.
ALTER TABLE relay_connector_subscriptions DROP CONSTRAINT relay_connector_subscriptions_event_check;
ALTER TABLE relay_connector_subscriptions ADD CONSTRAINT relay_connector_subscriptions_event_check
 CHECK(event ~ '^[a-z][a-z0-9_]{0,63}\.[a-z][a-z0-9_]{0,63}$'
  OR (provider='system' AND event ~ '^[a-z][a-z0-9_.]{0,63}$'));
-- The events key is kept only as its digest, and shown once when made.
ALTER TABLE relay_connected_systems ADD COLUMN events_key TEXT;
ALTER TABLE relay_connected_systems ADD COLUMN events_on BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE relay_connected_systems ADD CONSTRAINT relay_connected_systems_events
 CHECK(NOT events_on OR events_key IS NOT NULL);
-- Every event a system sent, for Recent events and duplicates: field names,
-- never values. Kept 30 days.
CREATE TABLE relay_system_events (
 workspace TEXT NOT NULL, connection TEXT NOT NULL,
 id TEXT NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
 type TEXT NOT NULL CHECK(type ~ '^[a-z][a-z0-9_.]{0,63}$'),
 subject TEXT CHECK(subject IS NULL OR length(subject) BETWEEN 1 AND 128),
 hash TEXT NOT NULL,
 fields JSONB NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(fields)='array'),
 matched INTEGER NOT NULL DEFAULT 0 CHECK(matched >= 0),
 test BOOLEAN NOT NULL DEFAULT FALSE,
 received_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,connection,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_connected_systems(workspace,connection)
);
CREATE INDEX relay_system_events_recent ON relay_system_events(workspace,connection,received_ms DESC);

-- Results back to the system (C4). An automation may name up to eight result
-- fields; the agent fills them with report_result, and Cuckoo checks their
-- names and kinds. A finished run without them counts as failed.
CREATE TABLE relay_system_automations (
 workspace TEXT NOT NULL, trigger_id TEXT NOT NULL, connection TEXT NOT NULL,
 results JSONB NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(results)='array' AND jsonb_array_length(results)<=8),
 PRIMARY KEY(workspace,trigger_id),
 FOREIGN KEY(workspace,trigger_id) REFERENCES relay_event_triggers(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_connected_systems(workspace,connection)
);
CREATE TABLE relay_system_results (
 workspace TEXT NOT NULL, reply TEXT NOT NULL,
 fields JSONB NOT NULL CHECK(jsonb_typeof(fields)='object' AND octet_length(fields::text)<=8192),
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,reply),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply)
);
