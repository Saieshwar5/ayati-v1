-- Rules belong to the agent, and admins set safety per connector action
-- (D231 slice 4, D233 Tools). Replaces per-conversation rules, per-person
-- delegation ceilings, designated approvers and the guided permission setup.
--
-- relay_connector_rules: the admin's rule per action, the loosest an agent may
--   use. A missing row means the safe default: reads Allow, writes Ask first.
-- relay_agent_connections: which connectors an agent uses (the Tools switch).
-- relay_control_rules: the agent's own setting, which can only be stricter.
-- The switch, the workspace rule and the agent rule each take a new value from
-- relay_control_revision when they change. An action's effective revision is the
-- greatest of the three for that action, so it only ever moves forward: rows are
-- deleted only when the connection is reset, which also removes the switch.
CREATE SEQUENCE relay_control_revision;
CREATE TABLE relay_connector_rules (
 workspace TEXT NOT NULL, connection TEXT NOT NULL, operation TEXT NOT NULL,
 mode TEXT NOT NULL CHECK(mode IN ('allow','ask','block')),
 needs_other BOOLEAN NOT NULL DEFAULT false, revision BIGINT NOT NULL,
 updated_by TEXT NOT NULL REFERENCES relay_users(id), updated_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,connection,operation),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id)
);
CREATE TABLE relay_agent_connections (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, connection TEXT NOT NULL,
 revision BIGINT NOT NULL, enabled_by TEXT NOT NULL REFERENCES relay_users(id), at BIGINT NOT NULL,
 PRIMARY KEY(workspace,agent,connection),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id)
);

-- Each workspace rule takes the strictest limit an admin had delegated for it.
INSERT INTO relay_connector_rules(workspace,connection,operation,mode,needs_other,revision,updated_by,updated_at)
SELECT d.workspace,d.connection,d.operation,
 CASE WHEN bool_or(d.ceiling='block') THEN 'block' WHEN bool_or(d.ceiling='ask') THEN 'ask' ELSE 'allow' END,
 false,nextval('relay_control_revision'),min(d.approver),extract(epoch FROM now())::bigint
FROM relay_control_delegations d GROUP BY d.workspace,d.connection,d.operation;
-- Agents keep the connectors they had rules for, and their own settings.
INSERT INTO relay_agent_connections(workspace,agent,connection,revision,enabled_by,at)
SELECT r.workspace,r.agent,r.connection,nextval('relay_control_revision'),min(r.configured_by),extract(epoch FROM now())::bigint
FROM relay_control_rules r WHERE r.mode<>'block' GROUP BY r.workspace,r.agent,r.connection;
-- "Ask first" goes to every manager of the agent; the first decision wins.
-- Admins may decide once nobody has for approval_backup_hours (D231).
ALTER TABLE relay_workspaces ADD COLUMN approval_backup_hours INTEGER NOT NULL DEFAULT 4
 CHECK(approval_backup_hours BETWEEN 1 AND 168);
ALTER TABLE relay_control_actions ADD COLUMN needs_other BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN decided_by TEXT REFERENCES relay_users(id), ADD COLUMN decided_at BIGINT;
-- Work waiting for approval under the old rules asks again under the new ones.
UPDATE relay_control_actions SET state='invalidated',
 result='"Permissions moved to the new tools and safety rules; ask again."'::jsonb
WHERE state='pending';

DROP VIEW relay_control_effective_rules;
ALTER TABLE relay_control_rules DROP COLUMN chat, DROP COLUMN approver, DROP COLUMN approval_chat,
 DROP COLUMN approver_revision, DROP COLUMN delegation_revision,
 ADD COLUMN changed BIGINT NOT NULL DEFAULT nextval('relay_control_revision');
DROP TABLE relay_control_delegations;
DROP TABLE relay_control_approvers;

CREATE FUNCTION relay_stricter(a TEXT, b TEXT) RETURNS TEXT LANGUAGE SQL IMMUTABLE AS $$
 SELECT CASE WHEN 'block' IN (a,b) THEN 'block' WHEN 'ask' IN (a,b) THEN 'ask' ELSE 'allow' END
$$;
-- What an agent may do with each action of each connector it uses. `approver`
-- is the agent's owner: hand-offs compare it, while decisions go to all managers.
CREATE VIEW relay_control_effective_rules AS
 SELECT x.workspace,x.agent,x.connection,x.operation,x.mode,x.needs_other,x.revision,x.approver FROM (
  SELECT ac.workspace,ac.agent,ac.connection,op->>'id' AS operation,
   relay_stricter(COALESCE(w.mode,CASE WHEN op->>'effect'='read' THEN 'allow' ELSE 'ask' END),COALESCE(r.mode,'allow')) AS mode,
   COALESCE(w.needs_other,false) AS needs_other,
   GREATEST(ac.revision,COALESCE(w.revision,0),COALESCE(r.changed,0)) AS revision,a.owner_id AS approver
  FROM relay_agent_connections ac
  JOIN relay_shared_agents a ON a.workspace=ac.workspace AND a.id=ac.agent
  JOIN relay_control_connections c ON c.workspace=ac.workspace AND c.id=ac.connection
  JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server
  CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op
  LEFT JOIN relay_connector_rules w ON w.workspace=ac.workspace AND w.connection=ac.connection AND w.operation=op->>'id'
  LEFT JOIN relay_control_rules r ON r.workspace=ac.workspace AND r.agent=ac.agent AND r.connection=ac.connection AND r.operation=op->>'id'
  WHERE s.approved AND c.enabled AND a.owner_id IS NOT NULL
 ) x WHERE x.mode<>'block';

-- Revoking event triggers and hand-offs follows the new tables.
CREATE OR REPLACE FUNCTION relay_event_revoke() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; affected text[]; pair record;
BEGIN
 IF TG_TABLE_NAME='relay_members' AND TG_OP='UPDATE' AND to_jsonb(NEW)->'role' IS NOT DISTINCT FROM to_jsonb(OLD)->'role' THEN RETURN NEW; END IF;
 item:=CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;w:=item->>'workspace';
 SELECT array_agg(t.id) INTO affected FROM relay_event_triggers t
 WHERE t.workspace=w AND CASE TG_TABLE_NAME
 WHEN 'relay_control_rules' THEN t.connection=item->>'connection' AND t.agent=item->>'agent' AND t.read_operation=item->>'operation'
 WHEN 'relay_control_connections' THEN t.connection=item->>'id'
 WHEN 'relay_control_systems' THEN EXISTS(SELECT 1 FROM relay_control_connections c WHERE c.workspace=w AND c.id=t.connection AND c.server=item->>'server')
 WHEN 'relay_connector_rules' THEN t.connection=item->>'connection' AND t.read_operation=item->>'operation'
 WHEN 'relay_agent_connections' THEN t.connection=item->>'connection' AND t.agent=item->>'agent'
 WHEN 'relay_members' THEN t.principal=item->>'user_id'
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
CREATE OR REPLACE FUNCTION relay_handoff_authority_changed() RETURNS trigger LANGUAGE plpgsql AS $$
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
 WHEN 'relay_connector_rules' THEN h.source->>'connection'=item->>'connection' AND h.source->>'operation'=item->>'operation'
 WHEN 'relay_agent_connections' THEN h.target=item->>'agent' AND h.source->>'connection'=item->>'connection'
 WHEN 'relay_members' THEN h.principal=item->>'user_id'
 ELSE FALSE END
 LOOP PERFORM relay_handoff_revoke_source(w,affected.root,affected.scope,affected.chat);END LOOP;
 -- Changing a sender ceiling never upgrades an already accepted delegation.
 FOR affected IN SELECT h.child FROM relay_agent_handoffs h WHERE h.workspace=w AND h.state='pending' AND (
 (TG_TABLE_NAME='relay_members' AND h.principal=item->>'user_id') OR
 EXISTS(SELECT 1 FROM jsonb_array_elements(h.controls) r WHERE CASE TG_TABLE_NAME
 WHEN 'relay_control_rules' THEN h.sender=item->>'agent' AND r->>'connection'=item->>'connection' AND r->>'operation'=item->>'operation'
 WHEN 'relay_control_connections' THEN r->>'connection'=item->>'id'
 WHEN 'relay_control_systems' THEN EXISTS(SELECT 1 FROM relay_control_connections c WHERE c.workspace=w AND c.id=r->>'connection' AND c.server=item->>'server')
 WHEN 'relay_connector_rules' THEN r->>'connection'=item->>'connection' AND r->>'operation'=item->>'operation'
 WHEN 'relay_agent_connections' THEN h.sender=item->>'agent' AND r->>'connection'=item->>'connection'
 WHEN 'relay_members' THEN r->>'approver'=item->>'user_id'
 ELSE FALSE END))
 LOOP PERFORM relay_handoff_cancel_branch(w,affected.child,'The sending authority changed; helper work was stopped.','cancelled');END LOOP;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;

CREATE TRIGGER relay_event_connector_rule_changed AFTER INSERT OR UPDATE OR DELETE ON relay_connector_rules FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_event_agent_connection_changed AFTER UPDATE OR DELETE ON relay_agent_connections FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_handoff_connector_rule_changed AFTER INSERT OR UPDATE OR DELETE ON relay_connector_rules FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_handoff_agent_connection_changed AFTER UPDATE OR DELETE ON relay_agent_connections FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_home_connector_rule_changed AFTER INSERT OR UPDATE OR DELETE ON relay_connector_rules FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_home_agent_connection_changed AFTER UPDATE OR DELETE ON relay_agent_connections FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_context_connector_rule AFTER INSERT OR UPDATE OR DELETE ON relay_connector_rules FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
CREATE TRIGGER relay_context_agent_connection AFTER UPDATE OR DELETE ON relay_agent_connections FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
-- An agent's first own rule can only make it stricter, so it revokes like a change.
CREATE TRIGGER relay_event_rule_added AFTER INSERT ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_event_revoke();
CREATE TRIGGER relay_handoff_rule_added AFTER INSERT ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_handoff_authority_changed();
CREATE TRIGGER relay_home_rule_added AFTER INSERT ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_home_changed();
CREATE TRIGGER relay_context_rule_added AFTER INSERT ON relay_control_rules FOR EACH ROW EXECUTE FUNCTION relay_context_changed();
