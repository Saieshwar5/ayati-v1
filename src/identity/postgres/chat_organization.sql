-- Personal navigation preferences never supply conversation or execution authority.
CREATE TABLE relay_chat_preferences (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 revision BIGINT NOT NULL DEFAULT 0 CHECK(revision>=0),
 pins JSONB NOT NULL DEFAULT '[]' CHECK(jsonb_typeof(pins)='array' AND jsonb_array_length(pins)<=30),
 last_request TEXT, last_payload JSONB,
 PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TRIGGER relay_chat_preferences_change AFTER INSERT OR UPDATE ON relay_chat_preferences
 FOR EACH ROW EXECUTE FUNCTION relay_record_change();

ALTER TABLE relay_chats ADD COLUMN archived BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN archive_revision BIGINT NOT NULL DEFAULT 0 CHECK(archive_revision>=0),
 ADD COLUMN archived_at BIGINT, ADD COLUMN archived_by TEXT,
 ADD COLUMN archive_request TEXT, ADD COLUMN archive_payload JSONB,
 ADD CONSTRAINT relay_no_archived_direct CHECK(NOT archived OR kind<>'direct');

-- A tripwire for new intake adapters. Existing progress, completed results and
-- approval-reference delivery retain their own source and exact-task checks.
CREATE FUNCTION relay_reject_archived_intake() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE closed BOOLEAN;
BEGIN
 IF NEW.kind='user' OR NEW.state='queued' THEN
  SELECT archived INTO closed FROM relay_chats WHERE workspace=NEW.workspace AND id=NEW.chat FOR SHARE;
  IF closed THEN RAISE EXCEPTION 'This channel is archived. Restore it before sending new messages.'; END IF;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_archived_intake BEFORE INSERT ON relay_chat_messages
 FOR EACH ROW EXECUTE FUNCTION relay_reject_archived_intake();
