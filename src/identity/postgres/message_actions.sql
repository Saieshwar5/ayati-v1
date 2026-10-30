-- People can edit or delete their own messages and react to any message they can
-- read. A deleted message keeps its place (threads stay intact) but loses its text
-- and files; the 'deleted' state already keeps it out of agent context and unread
-- counts. Agent work records its asking message in relay_chat_jobs.request_message.
ALTER TABLE relay_chat_messages ADD COLUMN edited_at BIGINT;

CREATE TABLE relay_message_reactions (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, message TEXT NOT NULL, user_id TEXT NOT NULL,
 emoji TEXT NOT NULL CHECK(char_length(emoji) BETWEEN 1 AND 8), at BIGINT NOT NULL,
 PRIMARY KEY(workspace,message,user_id,emoji),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id) ON DELETE CASCADE
);

-- Reaction changes refresh only the affected message on open screens.
CREATE OR REPLACE FUNCTION relay_record_change() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; c text; m text; n bigint;
BEGIN
 item := CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 w := item->>'workspace'; c := item->>'chat';
 IF TG_TABLE_NAME='relay_chats' THEN c := item->>'id'; END IF;
 IF TG_TABLE_NAME='relay_chat_messages' THEN m := item->>'id'; END IF;
 IF TG_TABLE_NAME IN ('relay_message_receipts','relay_message_reactions') THEN m := item->>'message'; END IF;
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events VALUES(w,n,c,m,CASE WHEN m IS NULL THEN 'access' ELSE 'message' END);
 PERFORM pg_notify('relay_message_changes',w);
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_reaction_change AFTER INSERT OR DELETE ON relay_message_reactions
 FOR EACH ROW EXECUTE FUNCTION relay_record_change();

-- Author edits/deletions and archiving do not fence existing replies or questions.
CREATE OR REPLACE FUNCTION relay_context_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb;w text;a text:='';c text:='';category_name text:='access';
BEGIN
 item:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;w:=item->>'workspace';
 IF TG_TABLE_NAME='relay_shared_agents' THEN a:=item->>'id';
 ELSIF TG_TABLE_NAME IN ('relay_agent_knowledge','relay_chat_access','relay_agent_maintainers') THEN a:=item->>'agent';
 ELSIF TG_TABLE_NAME IN ('relay_chat_people','relay_chat_agents','relay_chat_messages') THEN c:=item->>'chat';
 ELSIF TG_TABLE_NAME='relay_chats' THEN c:=item->>'id'; END IF;
 IF TG_TABLE_NAME='relay_agent_knowledge' THEN c:=COALESCE(item->>'chat',''); END IF;
 IF TG_TABLE_NAME IN ('relay_agent_knowledge','relay_shared_agents','relay_chat_messages') THEN category_name:='context'; END IF;
 IF TG_TABLE_NAME='relay_chat_messages' THEN
  IF OLD.state NOT IN ('sent','complete') THEN RETURN COALESCE(NEW,OLD); END IF;
  IF TG_OP='UPDATE' AND (NEW.content,NEW.state) IS NOT DISTINCT FROM (OLD.content,OLD.state) THEN RETURN NEW; END IF;
  -- An author editing or deleting their own words keeps answers already given in
  -- the same conversation; work the deleted message asked for is stopped separately.
  IF TG_OP='UPDATE' AND NEW.kind='user' AND NEW.edited_at IS DISTINCT FROM OLD.edited_at THEN RETURN NEW; END IF;
 END IF;
 -- Archiving or restoring a channel closes or reopens new intake; who can read
 -- it is unchanged, so its saved work and waiting questions stay available.
 IF TG_TABLE_NAME='relay_chats' AND TG_OP='UPDATE' AND
  (to_jsonb(NEW)-'archived'-'archive_revision'-'archived_at'-'archived_by'-'archive_request'-'archive_payload')
  = (to_jsonb(OLD)-'archived'-'archive_revision'-'archived_at'-'archived_by'-'archive_request'-'archive_payload') THEN RETURN NEW; END IF;
 INSERT INTO relay_context_epochs(workspace,category,agent,chat) VALUES(w,category_name,COALESCE(a,''),COALESCE(c,''))
 ON CONFLICT(workspace,category,agent,chat) DO UPDATE SET revision=relay_context_epochs.revision+1;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
