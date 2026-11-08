-- D246 (2026-11-07): an emoji-only message to an agent starts no model run; each
-- asked agent reacts to it instead ("Atlas reacted 👍"). People's reactions stay in
-- relay_message_reactions, keyed by member, unchanged. An agent reacts once per
-- message, and its reaction goes with the message.
CREATE TABLE relay_agent_reactions (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, message TEXT NOT NULL, agent TEXT NOT NULL,
 emoji TEXT NOT NULL CHECK(char_length(emoji) BETWEEN 1 AND 8), at BIGINT NOT NULL,
 PRIMARY KEY(workspace,message,agent),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id) ON DELETE CASCADE
);
-- An agent's reaction refreshes only its message on open screens, like a person's.
CREATE OR REPLACE FUNCTION relay_record_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; c text; m text; n bigint;
BEGIN
 item := CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 w := item->>'workspace'; c := item->>'chat';
 IF TG_TABLE_NAME='relay_chats' THEN c := item->>'id'; END IF;
 IF TG_TABLE_NAME='relay_chat_messages' THEN m := item->>'id'; END IF;
 IF TG_TABLE_NAME IN ('relay_message_receipts','relay_message_reactions','relay_agent_receipts','relay_agent_reactions') THEN m := item->>'message'; END IF;
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events VALUES(w,n,c,m,CASE WHEN m IS NULL THEN 'access' ELSE 'message' END);
 PERFORM pg_notify('relay_message_changes',w);
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_agent_reaction_change AFTER INSERT OR DELETE ON relay_agent_reactions
 FOR EACH ROW EXECUTE FUNCTION relay_record_change();
