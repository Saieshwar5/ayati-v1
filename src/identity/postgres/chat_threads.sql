-- Schema 115: threads beside the conversation. A thread's root carries its
-- reply count and last reply time (read through relay_messages_by_thread,
-- schema 2); a reply may also be sent to the conversation (`broadcast`), so
-- the main timeline shows it too.
ALTER TABLE relay_chat_messages ADD COLUMN broadcast BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE relay_chat_messages ADD CONSTRAINT relay_message_broadcast_in_thread CHECK(NOT broadcast OR thread IS NOT NULL);

-- A reply, or a reply's deletion, changes its root's summary: record a change
-- for the root so open screens refresh it, as relay_record_change does.
CREATE FUNCTION relay_thread_reply_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE n bigint;
BEGIN
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(NEW.workspace,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events(workspace,cursor,chat,message,kind) VALUES(NEW.workspace,n,NEW.chat,NEW.thread,'message');
 PERFORM pg_notify('relay_message_changes',NEW.workspace);
 RETURN NEW;
END $$;
CREATE TRIGGER relay_thread_reply_added AFTER INSERT ON relay_chat_messages
 FOR EACH ROW WHEN (NEW.thread IS NOT NULL AND NEW.update_of IS NULL) EXECUTE FUNCTION relay_thread_reply_changed();
CREATE TRIGGER relay_thread_reply_deleted AFTER UPDATE OF state ON relay_chat_messages
 FOR EACH ROW WHEN (NEW.thread IS NOT NULL AND NEW.update_of IS NULL AND NEW.state='deleted' AND OLD.state<>'deleted') EXECUTE FUNCTION relay_thread_reply_changed();
