-- Transactional per-workspace clocks impose commit ordering without a global lock.
CREATE TABLE relay_sync_clocks (workspace TEXT PRIMARY KEY REFERENCES relay_workspaces(id), cursor BIGINT NOT NULL DEFAULT 0);
CREATE TABLE relay_sync_events (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), cursor BIGINT NOT NULL,
 chat TEXT, message TEXT, kind TEXT NOT NULL, PRIMARY KEY(workspace,cursor)
);
CREATE TABLE relay_message_receipts (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, message TEXT NOT NULL, user_id TEXT NOT NULL,
 delivered_at BIGINT NOT NULL, seen_at BIGINT,
 PRIMARY KEY(workspace,message,user_id),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_receipts_by_reader ON relay_message_receipts(workspace,user_id,message);
CREATE FUNCTION relay_record_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; c text; m text; n bigint;
BEGIN
 item := CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 w := item->>'workspace'; c := item->>'chat';
 IF TG_TABLE_NAME='relay_chats' THEN c := item->>'id'; END IF;
 IF TG_TABLE_NAME='relay_chat_messages' THEN m := item->>'id'; END IF;
 IF TG_TABLE_NAME='relay_message_receipts' THEN m := item->>'message'; END IF;
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events VALUES(w,n,c,m,CASE WHEN m IS NULL THEN 'access' ELSE 'message' END);
 PERFORM pg_notify('relay_message_changes',w);
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_message_change AFTER INSERT OR UPDATE ON relay_chat_messages FOR EACH ROW EXECUTE FUNCTION relay_record_change();
CREATE TRIGGER relay_receipt_change AFTER INSERT OR UPDATE ON relay_message_receipts FOR EACH ROW EXECUTE FUNCTION relay_record_change();
CREATE TRIGGER relay_channel_change AFTER INSERT OR UPDATE ON relay_chats FOR EACH ROW EXECUTE FUNCTION relay_record_change();
CREATE TRIGGER relay_people_change AFTER INSERT OR DELETE ON relay_chat_people FOR EACH ROW EXECUTE FUNCTION relay_record_change();
CREATE TRIGGER relay_membership_change AFTER INSERT OR UPDATE OR DELETE ON relay_members FOR EACH ROW EXECUTE FUNCTION relay_record_change();
