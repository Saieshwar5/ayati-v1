-- Workspace roles become Admin and Member (D231). Owner becomes Admin and User
-- becomes Member; nothing else about anyone's access changes.
ALTER TABLE relay_members DROP CONSTRAINT relay_members_role_check;
ALTER TABLE relay_invitations DROP CONSTRAINT relay_invitations_role_check;
INSERT INTO relay_audit(actor,workspace,action,target,at)
SELECT user_id,workspace,'role_renamed',role || ' -> ' || CASE role WHEN 'Owner' THEN 'Admin' ELSE 'Member' END,extract(epoch FROM now())::bigint
FROM relay_members;
-- Role-change triggers block schedules, event triggers, hand-offs, samples and
-- saved context because access changed. A rename changes no one's access, so
-- they are off for this one statement.
ALTER TABLE relay_members DISABLE TRIGGER USER;
UPDATE relay_members SET role=CASE role WHEN 'Owner' THEN 'Admin' ELSE 'Member' END;
ALTER TABLE relay_members ENABLE TRIGGER USER;
UPDATE relay_invitations SET role='Member';
ALTER TABLE relay_members ADD CONSTRAINT relay_members_role_check CHECK(role IN ('Admin','Member'));
ALTER TABLE relay_invitations ADD CONSTRAINT relay_invitations_role_check CHECK(role='Member');
-- Both definitions compare role names, so they are recreated with the new names.
CREATE OR REPLACE VIEW relay_control_effective_rules AS
 SELECT r.* FROM relay_control_rules r
 JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection
 JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server
 JOIN relay_shared_agents a ON a.workspace=r.workspace AND a.id=r.agent
 JOIN relay_members m ON m.workspace=r.workspace AND m.user_id=r.configured_by
 JOIN relay_members p ON p.workspace=r.workspace AND p.user_id=r.approver
 LEFT JOIN relay_control_delegations d ON d.workspace=r.workspace AND d.connection=r.connection AND d.user_id=r.configured_by AND d.operation=r.operation
 LEFT JOIN relay_members dm ON dm.workspace=d.workspace AND dm.user_id=d.approver AND dm.role='Admin'
 LEFT JOIN relay_control_approvers g ON g.workspace=r.workspace AND g.connection=r.connection AND g.operation=r.operation AND g.user_id=r.approver
 LEFT JOIN relay_members gm ON gm.workspace=g.workspace AND gm.user_id=g.granted_by AND gm.role='Admin'
 WHERE s.approved AND c.enabled AND r.mode<>'block'
 AND ((p.role='Admin' AND r.approver_revision IS NULL) OR (r.mode='ask' AND g.enabled AND g.revision=r.approver_revision AND gm.user_id IS NOT NULL))
 AND ((m.role='Admin' AND r.delegation_revision IS NULL AND (r.mode='ask' OR r.configured_by=r.approver))
 OR (a.owner_id=r.configured_by AND r.delegation_revision=d.revision AND dm.user_id IS NOT NULL
 AND (r.approver=d.approver OR (r.mode='ask' AND g.granted_by=d.approver AND g.enabled AND g.revision=r.approver_revision)) AND (d.ceiling='allow' OR (d.ceiling='ask' AND r.mode='ask'))));
CREATE OR REPLACE FUNCTION relay_communication_current(w TEXT, c TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM relay_communication_conversations WHERE workspace=w AND chat=c)
 OR EXISTS(
    SELECT 1 FROM relay_communication_conversations v
    JOIN relay_communication_endpoints e ON e.workspace=v.workspace AND e.id=v.endpoint
    JOIN relay_communication_bindings b ON b.workspace=v.workspace AND b.endpoint=v.endpoint AND b.sender=v.sender
    JOIN relay_members m ON m.workspace=v.workspace AND m.user_id=b.user_id
    JOIN relay_shared_agents a ON a.workspace=v.workspace AND a.id=e.agent
    JOIN relay_chats ch ON ch.workspace=v.workspace AND ch.id=v.chat
    WHERE v.workspace=w AND v.chat=c AND e.enabled AND b.enabled
      AND e.revision=v.endpoint_revision AND b.revision=v.binding_revision
      AND ch.visibility='private' AND NOT ch.archived
      AND NOT COALESCE((a.settings->>'paused')::BOOLEAN,FALSE)
      AND (a.scope<>'private' OR a.owner_id=b.user_id)
      AND (m.role='Admin' OR a.owner_id=b.user_id OR EXISTS(
          SELECT 1 FROM relay_chat_access x WHERE x.workspace=w AND x.user_id=b.user_id AND x.agent=e.agent))
      AND (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=w AND p.chat=c)=1
      AND EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=w AND p.chat=c AND p.user_id=b.user_id)
      AND (SELECT count(*) FROM relay_chat_agents x WHERE x.workspace=w AND x.chat=c)=1
      AND EXISTS(SELECT 1 FROM relay_chat_agents x WHERE x.workspace=w AND x.chat=c AND x.agent=e.agent)
 )
$$;
