-- Versions are evidence, never grants. Epochs permanently fence revoked/regranted sources.
CREATE TABLE relay_context_epochs (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),agent TEXT NOT NULL DEFAULT '',chat TEXT NOT NULL DEFAULT '',
 category TEXT NOT NULL,revision BIGINT NOT NULL DEFAULT 1,PRIMARY KEY(workspace,category,agent,chat)
);
CREATE TABLE relay_context_attempts (
 workspace TEXT NOT NULL,task TEXT NOT NULL,claim TEXT NOT NULL,agent TEXT NOT NULL,chat TEXT NOT NULL,requester TEXT NOT NULL,
 generation JSONB NOT NULL,prepared JSONB NOT NULL,evidence JSONB NOT NULL,
 created_at BIGINT NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp()))::bigint,
 PRIMARY KEY(workspace,task,claim),FOREIGN KEY(workspace,task) REFERENCES relay_chat_jobs(workspace,reply),
 CHECK(octet_length(prepared::text)<=524288)
);
CREATE INDEX relay_context_attempts_conversation ON relay_context_attempts(workspace,agent,chat,requester,created_at DESC,task,claim);
CREATE TABLE relay_context_reads (
 workspace TEXT NOT NULL,task TEXT NOT NULL,claim TEXT NOT NULL,sequence BIGINT NOT NULL,
 tool TEXT NOT NULL,result JSONB NOT NULL,created_at BIGINT NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp()))::bigint,
 PRIMARY KEY(workspace,task,claim,sequence),FOREIGN KEY(workspace,task,claim) REFERENCES relay_context_attempts(workspace,task,claim),
 CHECK(sequence BETWEEN 1 AND 100),CHECK(octet_length(result::text)<=131072)
);
CREATE TABLE relay_working_context (
 workspace TEXT NOT NULL,agent TEXT NOT NULL,chat TEXT NOT NULL,requester TEXT NOT NULL,revision BIGINT NOT NULL,
 task TEXT NOT NULL,claim TEXT NOT NULL,generation JSONB NOT NULL,cutoff BIGINT NOT NULL,brief JSONB NOT NULL,
 created_at BIGINT NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp()))::bigint,
 PRIMARY KEY(workspace,agent,chat,requester,revision),FOREIGN KEY(workspace,task,claim) REFERENCES relay_context_attempts(workspace,task,claim),
 CHECK(octet_length(brief::text)<=24576),CHECK(revision>0)
);
CREATE FUNCTION relay_context_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'Saved context evidence is immutable'; END $$;
CREATE TRIGGER relay_context_attempt_immutable BEFORE UPDATE OR DELETE ON relay_context_attempts FOR EACH ROW EXECUTE FUNCTION relay_context_immutable();
CREATE TRIGGER relay_context_read_immutable BEFORE UPDATE OR DELETE ON relay_context_reads FOR EACH ROW EXECUTE FUNCTION relay_context_immutable();
CREATE TRIGGER relay_working_context_immutable BEFORE UPDATE OR DELETE ON relay_working_context FOR EACH ROW EXECUTE FUNCTION relay_context_immutable();
CREATE FUNCTION relay_context_changed() RETURNS trigger LANGUAGE plpgsql AS $$
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
 END IF;
 INSERT INTO relay_context_epochs(workspace,category,agent,chat) VALUES(w,category_name,COALESCE(a,''),COALESCE(c,''))
 ON CONFLICT(workspace,category,agent,chat) DO UPDATE SET revision=relay_context_epochs.revision+1;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
-- New notes and policy changes invalidate affected agent briefs. Membership/visibility changes
-- fence a whole conversation; connector/library changes conservatively fence the workspace.
CREATE TRIGGER relay_context_note AFTER INSERT OR UPDATE OR DELETE ON relay_agent_knowledge FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_profile AFTER UPDATE OR DELETE ON relay_shared_agents FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_chat AFTER UPDATE OR DELETE ON relay_chats FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_people AFTER UPDATE OR DELETE ON relay_chat_people FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_agents AFTER UPDATE OR DELETE ON relay_chat_agents FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_message AFTER UPDATE OR DELETE ON relay_chat_messages FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_access AFTER UPDATE OR DELETE ON relay_chat_access FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_member AFTER UPDATE OR DELETE ON relay_members FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_library AFTER UPDATE OR DELETE ON relay_library_items FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_library_access AFTER UPDATE OR DELETE ON relay_library_agents FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_rule AFTER UPDATE OR DELETE ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_connection AFTER UPDATE OR DELETE ON relay_control_connections FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_system AFTER UPDATE OR DELETE ON relay_control_systems FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_delegation AFTER UPDATE OR DELETE ON relay_control_delegations FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
