CREATE FUNCTION relay_handoff_revoke_source(w TEXT,tree TEXT,scope TEXT,c TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE tasks text[];
BEGIN
 UPDATE relay_agent_handoffs SET source_revoked=true WHERE workspace=w AND root=tree AND NOT source_revoked;
 SELECT array_agg(task) INTO tasks FROM (SELECT tree AS task UNION SELECT child FROM relay_agent_handoffs WHERE workspace=w AND root=tree) x;
 PERFORM relay_handoff_stop_tasks(w,tasks,'Task source access changed; this work cannot be revived.');
 PERFORM relay_event_access_changed(w,scope,c);
END $$;
CREATE FUNCTION relay_handoff_authority_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb;w text;affected record;
BEGIN
 IF TG_TABLE_NAME='relay_members' AND TG_OP='UPDATE' AND to_jsonb(NEW)->'role' IS NOT DISTINCT FROM to_jsonb(OLD)->'role' THEN RETURN NEW;END IF;
 item:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;w:=item->>'workspace';
 -- A returned helper result remains a source of the eventual parent response.
 -- Invalidate the entire bounded tree, including already saved derived views.
 FOR affected IN SELECT DISTINCT h.root,h.chat,h.source->>'source' AS scope FROM relay_agent_handoffs h
 WHERE h.workspace=w AND h.source IS NOT NULL AND NOT h.source_revoked AND CASE TG_TABLE_NAME
 WHEN 'relay_control_rules' THEN h.target=item->>'agent' AND h.source->>'connection'=item->>'connection' AND h.source->>'operation'=item->>'operation'
 WHEN 'relay_control_connections' THEN h.source->>'connection'=item->>'id'
 WHEN 'relay_control_systems' THEN EXISTS(SELECT 1 FROM relay_control_connections c WHERE c.workspace=w AND c.id=h.source->>'connection' AND c.server=item->>'server')
 WHEN 'relay_control_delegations' THEN h.principal=item->>'user_id' AND h.source->>'connection'=item->>'connection' AND h.source->>'operation'=item->>'operation'
 WHEN 'relay_members' THEN h.principal=item->>'user_id' OR EXISTS(SELECT 1 FROM relay_control_rules r WHERE r.workspace=w AND r.agent=h.target AND r.connection=h.source->>'connection' AND r.operation=h.source->>'operation' AND (r.configured_by=item->>'user_id' OR r.approver=item->>'user_id'))
 ELSE FALSE END
 LOOP PERFORM relay_handoff_revoke_source(w,affected.root,affected.scope,affected.chat);END LOOP;
 -- Changing a sender ceiling never upgrades an already accepted delegation.
 FOR affected IN SELECT h.child FROM relay_agent_handoffs h WHERE h.workspace=w AND h.state='pending' AND (
 (TG_TABLE_NAME='relay_members' AND h.principal=item->>'user_id') OR
 EXISTS(SELECT 1 FROM jsonb_array_elements(h.controls) r WHERE CASE TG_TABLE_NAME
 WHEN 'relay_control_rules' THEN h.sender=item->>'agent' AND r->>'connection'=item->>'connection' AND r->>'operation'=item->>'operation'
 WHEN 'relay_control_connections' THEN r->>'connection'=item->>'id'
 WHEN 'relay_control_systems' THEN EXISTS(SELECT 1 FROM relay_control_connections c WHERE c.workspace=w AND c.id=r->>'connection' AND c.server=item->>'server')
 WHEN 'relay_control_delegations' THEN r->>'connection'=item->>'connection' AND r->>'operation'=item->>'operation' AND r->>'configured_by'=item->>'user_id'
 WHEN 'relay_members' THEN r->>'configured_by'=item->>'user_id' OR r->>'approver'=item->>'user_id'
 ELSE FALSE END))
 LOOP PERFORM relay_handoff_cancel_branch(w,affected.child,'The sending authority changed; helper work was stopped.','cancelled');END LOOP;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_handoff_rule_changed AFTER UPDATE OR DELETE ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_handoff_connection_changed AFTER UPDATE OR DELETE ON relay_control_connections FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_handoff_system_changed AFTER UPDATE OR DELETE ON relay_control_systems FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_handoff_delegation_changed AFTER UPDATE OR DELETE ON relay_control_delegations FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_handoff_member_changed AFTER DELETE OR UPDATE OF role ON relay_members FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
