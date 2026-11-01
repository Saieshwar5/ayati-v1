-- Communication gateway (schema 54): real transports beside the local
-- simulations. An endpoint names the adapter that speaks its transport; a
-- developer transport (`mcp`) also names the operator-configured server that
-- bridges it. Simulations keep their email/WhatsApp names and no server.
ALTER TABLE relay_communication_endpoints
 DROP CONSTRAINT relay_communication_endpoints_transport_check,
 DROP CONSTRAINT relay_communication_endpoints_simulated_check,
 DROP CONSTRAINT relay_communication_endpoints_transport_address_key;
ALTER TABLE relay_communication_endpoints
 ADD COLUMN server TEXT CHECK(server IS NULL OR (length(server) BETWEEN 1 AND 128 AND server !~ '[[:cntrl:]]'));
ALTER TABLE relay_communication_endpoints
 ADD CONSTRAINT relay_communication_endpoints_transport_name CHECK(transport ~ '^[a-z][a-z0-9_]{0,31}$'),
 ADD CONSTRAINT relay_communication_endpoints_simulations CHECK(NOT simulated OR (transport IN ('email','whatsapp') AND server IS NULL));
-- An address belongs to one agent: across Cuckoo for a transport's own
-- addresses, within the workspace for a server's (each server serves one).
CREATE UNIQUE INDEX relay_communication_endpoints_address ON relay_communication_endpoints(transport,address) WHERE server IS NULL;
CREATE UNIQUE INDEX relay_communication_endpoints_server_address ON relay_communication_endpoints(workspace,transport,server,address) WHERE server IS NOT NULL;

ALTER TABLE relay_communication_inbox
 DROP CONSTRAINT relay_communication_inbox_transport_check,
 ADD CONSTRAINT relay_communication_inbox_transport_name CHECK(transport ~ '^[a-z][a-z0-9_]{0,31}$');
-- Why a send failed or is unclear, in Cuckoo's words or the platform's (bounded).
ALTER TABLE relay_communication_outbox
 DROP CONSTRAINT relay_communication_outbox_transport_check,
 ADD CONSTRAINT relay_communication_outbox_transport_name CHECK(transport ~ '^[a-z][a-z0-9_]{0,31}$'),
 ADD COLUMN reason TEXT CHECK(reason IS NULL OR length(reason)<=300);
CREATE INDEX relay_communication_outbox_pending ON relay_communication_outbox(state,lease_until)
 WHERE state IN ('queued','sending','uncertain');

-- Same rule as schema 32, now also letting a send record its reason.
CREATE OR REPLACE FUNCTION relay_communication_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE mutable TEXT[];
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Communication evidence cannot be deleted'; END IF;
 mutable:=CASE TG_TABLE_NAME
   WHEN 'relay_communication_outbox' THEN ARRAY['state','attempt','provider_id','lease_until','attempts','reason']
   WHEN 'relay_communication_endpoints' THEN ARRAY['enabled','revision']
   WHEN 'relay_communication_bindings' THEN ARRAY['enabled','revision']
   ELSE ARRAY[]::TEXT[] END;
 IF (to_jsonb(NEW)-mutable) IS DISTINCT FROM (to_jsonb(OLD)-mutable) THEN
   RAISE EXCEPTION 'Communication identity and envelope are immutable';
 END IF;
 RETURN NEW;
END $$;

-- How far the gateway has read each transport server. Events are saved before
-- the cursor moves, so a crash rereads them and deduplication drops repeats.
CREATE TABLE relay_communication_cursors (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 transport TEXT NOT NULL CHECK(transport ~ '^[a-z][a-z0-9_]{0,31}$'),
 server TEXT NOT NULL CHECK(length(server) BETWEEN 1 AND 128),
 cursor TEXT NOT NULL CHECK(length(cursor) BETWEEN 1 AND 256 AND cursor !~ '[[:cntrl:]]'),
 updated BIGINT NOT NULL,
 PRIMARY KEY(workspace,transport,server)
);
