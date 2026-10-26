CREATE TABLE relay_home_panels (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),id TEXT NOT NULL,owner_id TEXT NOT NULL REFERENCES relay_users(id),
 agent TEXT NOT NULL,chat TEXT NOT NULL,revision BIGINT NOT NULL,active_revision BIGINT,created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 CHECK(revision BETWEEN 1 AND 100),CHECK(active_revision IS NULL OR active_revision BETWEEN 1 AND revision)
);
CREATE INDEX relay_home_panels_owner ON relay_home_panels(workspace,owner_id,created_ms,id);
CREATE TABLE relay_home_panel_versions (
 workspace TEXT NOT NULL,panel TEXT NOT NULL,revision BIGINT NOT NULL,task TEXT NOT NULL,request_hash TEXT NOT NULL,
 definition JSONB NOT NULL,provenance JSONB NOT NULL,
 saved_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,panel,revision),UNIQUE(workspace,task,request_hash),
 FOREIGN KEY(workspace,panel) REFERENCES relay_home_panels(workspace,id),FOREIGN KEY(workspace,task) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE TABLE relay_home_layouts (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),owner_id TEXT NOT NULL REFERENCES relay_users(id),revision BIGINT NOT NULL CHECK(revision>0),data JSONB NOT NULL,
 PRIMARY KEY(workspace,owner_id)
);
CREATE TABLE relay_home_receipts (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),owner_id TEXT NOT NULL REFERENCES relay_users(id),request TEXT NOT NULL,input_hash TEXT NOT NULL,result JSONB NOT NULL,
 PRIMARY KEY(workspace,owner_id,request)
);
CREATE FUNCTION relay_home_version_immutable() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'Saved Home snapshots and receipts are immutable'; END $$;
CREATE TRIGGER relay_home_version_immutable BEFORE UPDATE OR DELETE ON relay_home_panel_versions FOR EACH ROW EXECUTE FUNCTION relay_home_version_immutable();
CREATE TRIGGER relay_home_receipt_immutable BEFORE UPDATE OR DELETE ON relay_home_receipts FOR EACH ROW EXECUTE FUNCTION relay_home_version_immutable();
CREATE FUNCTION relay_home_identity_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN IF (NEW.workspace,NEW.id,NEW.owner_id,NEW.agent,NEW.chat,NEW.created_ms) IS DISTINCT FROM (OLD.workspace,OLD.id,OLD.owner_id,OLD.agent,OLD.chat,OLD.created_ms) THEN RAISE EXCEPTION 'Home panel identity is immutable';END IF;RETURN NEW;END $$;
CREATE TRIGGER relay_home_identity_immutable BEFORE UPDATE ON relay_home_panels FOR EACH ROW EXECUTE FUNCTION relay_home_identity_immutable();
-- Personal invalidation has no private IDs, titles or payloads on the wire.
CREATE FUNCTION relay_home_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb;w text;person text;n bigint;
BEGIN
 item:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;w:=item->>'workspace';
 FOR person IN SELECT DISTINCT owner_id FROM relay_home_panels WHERE workspace=w AND
  (TG_TABLE_NAME NOT IN ('relay_home_panels','relay_home_layouts') OR owner_id=item->>'owner_id')
 LOOP
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1) ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events(workspace,cursor,kind,audience) VALUES(w,n,'panel_access',person);
 END LOOP;
 IF n IS NOT NULL THEN PERFORM pg_notify('relay_message_changes',w);END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_home_changed AFTER INSERT OR UPDATE ON relay_home_panels FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_layout_changed AFTER INSERT OR UPDATE ON relay_home_layouts FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_rule_changed AFTER UPDATE OR DELETE ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_connection_changed AFTER UPDATE OR DELETE ON relay_control_connections FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_system_changed AFTER UPDATE OR DELETE ON relay_control_systems FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_delegation_changed AFTER UPDATE OR DELETE ON relay_control_delegations FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_library_changed AFTER UPDATE OR DELETE ON relay_library_items FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_library_grant_changed AFTER DELETE OR UPDATE ON relay_library_agents FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
