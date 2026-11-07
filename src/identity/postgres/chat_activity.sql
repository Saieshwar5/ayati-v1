-- When each conversation last had activity, for the Chat list's order (schema 96).
-- Every column is added without rewriting its table: nullable, or a constant
-- default stored once for existing rows, then the clock as the default for new ones.

-- An agent's reply is created, queued, when its request is sent and keeps that
-- time in `at`; this records when a message became sent or finished later.
ALTER TABLE relay_chat_messages ADD COLUMN activity_at BIGINT;
CREATE INDEX relay_messages_activity ON relay_chat_messages(workspace,chat,activity_at)
 WHERE activity_at IS NOT NULL;

-- When a conversation was created, and when a person was added to one. Existing
-- rows keep 0, so the upgrade leaves the existing order to their messages.
ALTER TABLE relay_chats ADD COLUMN created_ms BIGINT NOT NULL DEFAULT 0;
ALTER TABLE relay_chats ALTER COLUMN created_ms SET DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint;
ALTER TABLE relay_chat_people ADD COLUMN added_ms BIGINT NOT NULL DEFAULT 0;
ALTER TABLE relay_chat_people ALTER COLUMN added_ms SET DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint;

-- Only messages people see as activity: never an agent's progress notes, and
-- failed, cancelled or queued replies do not count until they are sent or finished.
CREATE FUNCTION relay_message_activity() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
  IF NEW.update_of IS NULL AND NEW.state IN ('sent','complete') AND OLD.state NOT IN ('sent','complete') THEN
    NEW.activity_at := extract(epoch FROM clock_timestamp())::bigint;
  END IF;
  RETURN NEW;
END $$;
CREATE TRIGGER relay_message_activity BEFORE UPDATE OF state ON relay_chat_messages
 FOR EACH ROW EXECUTE FUNCTION relay_message_activity();
