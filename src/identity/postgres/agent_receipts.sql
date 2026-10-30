-- Delivered/seen for agents (people use relay_message_receipts, which is keyed
-- by workspace member). Delivered: the message became work for the agent.
-- Seen: an attempt started reading it. Rows live and go with their message.
CREATE TABLE relay_agent_receipts (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, message TEXT NOT NULL, agent TEXT NOT NULL,
 delivered_at BIGINT NOT NULL, seen_at BIGINT,
 PRIMARY KEY(workspace,message,agent),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id) ON DELETE CASCADE
);
CREATE OR REPLACE FUNCTION relay_record_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; c text; m text; n bigint;
BEGIN
 item := CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 w := item->>'workspace'; c := item->>'chat';
 IF TG_TABLE_NAME='relay_chats' THEN c := item->>'id'; END IF;
 IF TG_TABLE_NAME='relay_chat_messages' THEN m := item->>'id'; END IF;
 IF TG_TABLE_NAME IN ('relay_message_receipts','relay_message_reactions','relay_agent_receipts') THEN m := item->>'message'; END IF;
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events VALUES(w,n,c,m,CASE WHEN m IS NULL THEN 'access' ELSE 'message' END);
 PERFORM pg_notify('relay_message_changes',w);
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_agent_receipt_change AFTER INSERT OR UPDATE ON relay_agent_receipts
 FOR EACH ROW EXECUTE FUNCTION relay_record_change();
