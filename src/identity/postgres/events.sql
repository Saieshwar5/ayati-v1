CREATE TABLE relay_event_triggers (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, principal TEXT NOT NULL,
 source TEXT NOT NULL, binding TEXT NOT NULL, source_definition JSONB NOT NULL,
 connection TEXT NOT NULL, read_operation TEXT NOT NULL, chat TEXT NOT NULL,
 input JSONB NOT NULL, request_hash TEXT NOT NULL,
 connection_revision BIGINT NOT NULL, read_revision BIGINT NOT NULL, delegation_revision BIGINT,
 state TEXT NOT NULL DEFAULT 'enabled' CHECK(state IN ('enabled','paused','blocked')),
 revision BIGINT NOT NULL DEFAULT 1, detail TEXT NOT NULL DEFAULT '',
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id), FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id),
 FOREIGN KEY(principal) REFERENCES relay_users(id)
);
CREATE INDEX relay_event_trigger_source ON relay_event_triggers(workspace,source,state);
CREATE TABLE relay_event_inbox (
 workspace TEXT NOT NULL, source TEXT NOT NULL, id TEXT NOT NULL, fingerprint TEXT NOT NULL,
 binding TEXT NOT NULL, data JSONB CHECK(octet_length(data::text)<=8192),
 received_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 sequence BIGINT GENERATED ALWAYS AS IDENTITY,
 PRIMARY KEY(workspace,source,id), FOREIGN KEY(workspace) REFERENCES relay_workspaces(id)
);
CREATE TABLE relay_event_deliveries (
 workspace TEXT NOT NULL, id TEXT NOT NULL, trigger_id TEXT NOT NULL, source TEXT NOT NULL, event_id TEXT NOT NULL,
 authority JSONB NOT NULL, state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','cancelled')),
 message TEXT UNIQUE, reply TEXT UNIQUE, revoked BOOLEAN NOT NULL DEFAULT FALSE,
 detail TEXT NOT NULL DEFAULT '', checked_ms BIGINT NOT NULL DEFAULT 0,
 PRIMARY KEY(workspace,id), UNIQUE(workspace,trigger_id,event_id),
 FOREIGN KEY(workspace,trigger_id) REFERENCES relay_event_triggers(workspace,id),
 FOREIGN KEY(workspace,source,event_id) REFERENCES relay_event_inbox(workspace,source,id),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply),
 CHECK((state='accepted')=(reply IS NOT NULL)), CHECK((message IS NULL)=(reply IS NULL))
);
CREATE INDEX relay_event_pending ON relay_event_deliveries(checked_ms,id) WHERE state='pending';
CREATE INDEX relay_event_trigger_pending ON relay_event_deliveries(workspace,trigger_id,event_id) WHERE state='pending';
CREATE INDEX relay_event_reply ON relay_event_deliveries(workspace,reply) WHERE reply IS NOT NULL;
CREATE TABLE relay_event_trigger_changes (
 workspace TEXT NOT NULL, trigger_id TEXT NOT NULL, request TEXT NOT NULL, actor TEXT NOT NULL,
 payload JSONB NOT NULL, result JSONB NOT NULL, PRIMARY KEY(workspace,trigger_id,request),
 FOREIGN KEY(workspace,trigger_id) REFERENCES relay_event_triggers(workspace,id)
);
CREATE FUNCTION relay_event_trigger_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.workspace,NEW.id,NEW.agent,NEW.principal,NEW.source,NEW.binding,NEW.source_definition,
 NEW.connection,NEW.read_operation,NEW.chat,NEW.input,NEW.request_hash,NEW.created_ms)
 IS DISTINCT FROM ROW(OLD.workspace,OLD.id,OLD.agent,OLD.principal,OLD.source,OLD.binding,OLD.source_definition,
 OLD.connection,OLD.read_operation,OLD.chat,OLD.input,OLD.request_hash,OLD.created_ms) THEN
  RAISE EXCEPTION 'A saved event trigger configuration cannot change';
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER relay_event_trigger_immutable BEFORE UPDATE ON relay_event_triggers
 FOR EACH ROW EXECUTE FUNCTION relay_event_trigger_immutable();
-- Existing message transport carries only source/chat identities, never payloads.
CREATE FUNCTION relay_event_access_changed(w TEXT,s TEXT,c TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE n bigint;
BEGIN
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events(workspace,cursor,kind,chat,message) VALUES(w,n,'event_access',c,s);
 PERFORM pg_notify('relay_message_changes',w);
END $$;
CREATE FUNCTION relay_event_revoke() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; affected text[]; pair record;
BEGIN
 IF TG_TABLE_NAME='relay_members' AND TG_OP='UPDATE' AND to_jsonb(NEW)->'role' IS NOT DISTINCT FROM to_jsonb(OLD)->'role' THEN RETURN NEW; END IF;
 item:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;w:=item->>'workspace';
 SELECT array_agg(t.id) INTO affected FROM relay_event_triggers t
 WHERE t.workspace=w AND CASE TG_TABLE_NAME
 WHEN 'relay_control_rules' THEN t.connection=item->>'connection' AND t.agent=item->>'agent' AND t.read_operation=item->>'operation'
 WHEN 'relay_control_connections' THEN t.connection=item->>'id'
 WHEN 'relay_control_systems' THEN EXISTS(SELECT 1 FROM relay_control_connections c WHERE c.workspace=w AND c.id=t.connection AND c.server=item->>'server')
 WHEN 'relay_control_delegations' THEN t.connection=item->>'connection' AND t.read_operation=item->>'operation' AND t.principal=item->>'user_id'
 WHEN 'relay_members' THEN t.principal=item->>'user_id' OR EXISTS(SELECT 1 FROM relay_control_rules r WHERE r.workspace=w AND r.connection=t.connection AND r.operation=t.read_operation AND r.agent=t.agent AND (r.configured_by=item->>'user_id' OR r.approver=item->>'user_id')) OR EXISTS(SELECT 1 FROM relay_control_delegations d WHERE d.workspace=w AND d.connection=t.connection AND d.operation=t.read_operation AND d.user_id=t.principal AND d.approver=item->>'user_id')
 ELSE FALSE END;
 IF affected IS NULL THEN RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END; END IF;
 UPDATE relay_event_triggers SET state='blocked',revision=revision+1,
 detail='Source or membership authority changed. Review current access before resuming.'
 WHERE workspace=w AND id=ANY(affected) AND state<>'blocked';
 UPDATE relay_event_deliveries SET revoked=TRUE,state=CASE WHEN state='pending' THEN 'cancelled' ELSE state END,
 detail='Source authority changed; this delivery cannot be revived.'
 WHERE workspace=w AND trigger_id=ANY(affected);
 UPDATE relay_chat_jobs j SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE j.workspace=w AND j.state<>'finished' AND EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=w AND d.reply=j.reply AND d.revoked);
 UPDATE relay_chat_messages m SET state='cancelled',content='Request cancelled because source access changed.',progress=NULL
 WHERE m.workspace=w AND m.state='queued' AND EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=w AND d.reply=m.id AND d.revoked);
 UPDATE relay_control_actions a SET state='invalidated',result='"Source access changed; this pending operation was not dispatched."'::jsonb
 WHERE a.workspace=w AND a.state='pending' AND EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=w AND d.reply=a.job AND d.revoked);
 UPDATE relay_worker_assignments a SET state='finished',outcome='cancelled',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE a.workspace=w AND a.state IN ('launching','running') AND EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=w AND d.reply=a.reply AND d.revoked);
 UPDATE relay_assignment_credentials c SET context=NULL,expires=0 WHERE EXISTS(SELECT 1 FROM relay_worker_assignments a JOIN relay_event_deliveries d ON d.workspace=a.workspace AND d.reply=a.reply WHERE a.id=c.assignment AND a.workspace=w AND d.revoked);
 FOR pair IN SELECT DISTINCT source,chat FROM relay_event_triggers WHERE workspace=w AND id=ANY(affected)
 LOOP PERFORM relay_event_access_changed(w,pair.source,pair.chat); END LOOP;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_event_rule_changed AFTER UPDATE OR DELETE ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_event_connection_changed AFTER UPDATE OR DELETE ON relay_control_connections FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_event_system_changed AFTER UPDATE OR DELETE ON relay_control_systems FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_event_delegation_changed AFTER UPDATE OR DELETE ON relay_control_delegations FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_event_member_changed AFTER DELETE OR UPDATE OF role ON relay_members FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();

CREATE FUNCTION relay_event_delivery_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.workspace,NEW.id,NEW.trigger_id,NEW.source,NEW.event_id,NEW.authority)
 IS DISTINCT FROM ROW(OLD.workspace,OLD.id,OLD.trigger_id,OLD.source,OLD.event_id,OLD.authority)
 OR (OLD.revoked AND NOT NEW.revoked)
 OR (OLD.state<>'pending' AND (NEW.state IS DISTINCT FROM OLD.state OR NEW.message IS DISTINCT FROM OLD.message OR NEW.reply IS DISTINCT FROM OLD.reply)) THEN
  RAISE EXCEPTION 'Event provenance and terminal decisions cannot be rewritten';
 END IF; RETURN NEW;
END $$;
CREATE TRIGGER relay_event_delivery_immutable BEFORE UPDATE ON relay_event_deliveries FOR EACH ROW EXECUTE FUNCTION relay_event_delivery_immutable();
