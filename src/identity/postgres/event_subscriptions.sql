-- Provider events (schema 109). The controller subscribes a connected account
-- to a catalog app event at its provider; each subscription is an event source
-- beside the operator file, shared by every trigger with the same account,
-- event and settings. Signing keys are sealed with AYATI_SECRETS_KEY_FILE.
CREATE TABLE relay_connector_subscriptions (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 id TEXT NOT NULL,
 connection TEXT NOT NULL,
 event TEXT NOT NULL CHECK(event ~ '^[a-z][a-z0-9_]{0,63}\.[a-z][a-z0-9_]{0,63}$'),
 provider TEXT NOT NULL CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 provider_id TEXT NOT NULL CHECK(length(provider_id) BETWEEN 1 AND 200),
 trigger TEXT NOT NULL CHECK(length(trigger) BETWEEN 1 AND 200),
 config JSONB NOT NULL CHECK(jsonb_typeof(config)='object' AND octet_length(config::text)<=2000),
 config_hash TEXT NOT NULL,
 -- Cuckoo field name to the provider's payload field name.
 fields JSONB NOT NULL CHECK(jsonb_typeof(fields)='object' AND octet_length(fields::text)<=2000),
 -- The connection's provider identity when subscribed (never a token).
 binding JSONB NOT NULL CHECK(octet_length(binding::text)<=2000),
 connection_fingerprint TEXT NOT NULL,
 read_operation TEXT NOT NULL,
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 160),
 signing_key BYTEA CHECK(signing_key IS NULL OR octet_length(signing_key)<=8192),
 state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','broken','removed')),
 detail TEXT NOT NULL DEFAULT '' CHECK(length(detail)<=300),
 -- Removed at the provider too; retried with backoff until it is.
 released BOOLEAN NOT NULL DEFAULT FALSE,
 attempts INTEGER NOT NULL DEFAULT 0,
 retry_ms BIGINT NOT NULL DEFAULT 0,
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 changed_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id),
 CHECK(state<>'active' OR NOT released)
);
CREATE UNIQUE INDEX relay_connector_subscription_active
 ON relay_connector_subscriptions(workspace,connection,event,config_hash) WHERE state='active';
CREATE UNIQUE INDEX relay_connector_subscription_provider
 ON relay_connector_subscriptions(provider,provider_id) WHERE state='active';
CREATE INDEX relay_connector_subscription_lookup ON relay_connector_subscriptions(provider,provider_id);
CREATE INDEX relay_connector_subscription_release
 ON relay_connector_subscriptions(retry_ms) WHERE state<>'active' AND NOT released;

-- A provider's project-wide delivery setup, such as Composio's one webhook
-- subscription per project. The signing secret is sealed; it is never returned.
CREATE TABLE relay_connector_event_setup (
 provider TEXT PRIMARY KEY CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 provider_id TEXT NOT NULL CHECK(length(provider_id) BETWEEN 1 AND 200),
 webhook_url TEXT NOT NULL CHECK(length(webhook_url) BETWEEN 1 AND 500),
 events JSONB NOT NULL CHECK(jsonb_typeof(events)='array'),
 secret BYTEA NOT NULL CHECK(octet_length(secret)<=8192),
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed BIGINT NOT NULL
);

-- A trigger can now be removed. Removal is final: later authority changes,
-- which block every trigger on a connection, leave a removed one removed.
ALTER TABLE relay_event_triggers DROP CONSTRAINT relay_event_triggers_state_check;
ALTER TABLE relay_event_triggers ADD CONSTRAINT relay_event_triggers_state_check
 CHECK(state IN ('enabled','paused','blocked','removed'));
CREATE FUNCTION relay_event_trigger_removed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF OLD.state='removed' THEN
  NEW.state:='removed'; NEW.revision:=OLD.revision; NEW.detail:=OLD.detail;
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER relay_event_trigger_removed BEFORE UPDATE ON relay_event_triggers
 FOR EACH ROW EXECUTE FUNCTION relay_event_trigger_removed();

-- A disconnected account's subscriptions are removed; a reconnected one with a
-- different provider account breaks them. Either way its triggers were already
-- blocked by relay_event_revoke (which fires first, by name); this says why,
-- for Needs fixing. The provider side is cleaned up by the controller.
CREATE FUNCTION relay_subscription_connection_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE why text; next_state text;
BEGIN
 IF NOT NEW.enabled THEN
  next_state:='removed';
  why:='The app account was disconnected, so this app event stopped. Connect the account and create the trigger again.';
 ELSIF NEW.binding IS DISTINCT FROM OLD.binding OR NEW.provider IS DISTINCT FROM OLD.provider THEN
  next_state:='broken';
  why:='The app account was reconnected, so this app event stopped. Create the trigger again from the app''s event.';
 ELSE
  -- The same provider account: new triggers may keep using the subscription.
  UPDATE relay_connector_subscriptions SET connection_fingerprint=NEW.fingerprint,
   changed_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
  WHERE workspace=NEW.workspace AND connection=NEW.id AND state='active'
   AND connection_fingerprint IS DISTINCT FROM NEW.fingerprint;
  RETURN NEW;
 END IF;
 UPDATE relay_event_triggers SET detail=why
 WHERE workspace=NEW.workspace AND state<>'removed' AND source IN
  (SELECT id FROM relay_connector_subscriptions WHERE workspace=NEW.workspace AND connection=NEW.id AND state='active');
 UPDATE relay_connector_subscriptions SET state=next_state,detail=why,
  changed_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE workspace=NEW.workspace AND connection=NEW.id AND state='active';
 RETURN NEW;
END $$;
CREATE TRIGGER relay_subscription_connection_changed AFTER UPDATE ON relay_control_connections
 FOR EACH ROW EXECUTE FUNCTION relay_subscription_connection_changed();
