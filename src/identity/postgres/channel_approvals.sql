CREATE TABLE relay_control_approvers (
 workspace TEXT NOT NULL, connection TEXT NOT NULL, operation TEXT NOT NULL,
 user_id TEXT NOT NULL REFERENCES relay_users(id), enabled BOOLEAN NOT NULL,
 granted_by TEXT NOT NULL REFERENCES relay_users(id), revision BIGINT NOT NULL,
 PRIMARY KEY(workspace,connection,operation,user_id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id)
);
ALTER TABLE relay_control_rules ADD COLUMN approval_chat TEXT;
ALTER TABLE relay_control_rules ADD FOREIGN KEY(workspace,approval_chat) REFERENCES relay_chats(workspace,id);
ALTER TABLE relay_control_rules ADD COLUMN approver_revision BIGINT;
ALTER TABLE relay_control_actions ADD COLUMN approval_chat TEXT;
ALTER TABLE relay_control_actions ADD UNIQUE(workspace,id);
CREATE TABLE relay_control_deliveries (
 workspace TEXT NOT NULL, action TEXT NOT NULL, chat TEXT, message TEXT,
 status TEXT NOT NULL CHECK(status IN ('channel','inbox')), reason TEXT,
 PRIMARY KEY(workspace,action), UNIQUE(message),
 FOREIGN KEY(workspace,action) REFERENCES relay_control_actions(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id)
);
CREATE FUNCTION relay_approval_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE relay_chat_messages SET revision=revision+1
 WHERE workspace=NEW.workspace AND id IN (SELECT message FROM relay_control_deliveries WHERE workspace=NEW.workspace AND action=NEW.id);
 RETURN NEW;
END $$;
CREATE TRIGGER relay_approval_change AFTER UPDATE ON relay_control_actions FOR EACH ROW EXECUTE FUNCTION relay_approval_changed();
CREATE OR REPLACE VIEW relay_control_effective_rules AS
 SELECT r.* FROM relay_control_rules r
 JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection
 JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server
 JOIN relay_shared_agents a ON a.workspace=r.workspace AND a.id=r.agent
 JOIN relay_members m ON m.workspace=r.workspace AND m.user_id=r.configured_by
 JOIN relay_members p ON p.workspace=r.workspace AND p.user_id=r.approver
 LEFT JOIN relay_control_delegations d ON d.workspace=r.workspace AND d.connection=r.connection AND d.user_id=r.configured_by AND d.operation=r.operation
 LEFT JOIN relay_members dm ON dm.workspace=d.workspace AND dm.user_id=d.approver AND dm.role='Owner'
 LEFT JOIN relay_control_approvers g ON g.workspace=r.workspace AND g.connection=r.connection AND g.operation=r.operation AND g.user_id=r.approver
 LEFT JOIN relay_members gm ON gm.workspace=g.workspace AND gm.user_id=g.granted_by AND gm.role='Owner'
 WHERE s.approved AND c.enabled AND r.mode<>'block'
 AND ((p.role='Owner' AND r.approver_revision IS NULL) OR (r.mode='ask' AND g.enabled AND g.revision=r.approver_revision AND gm.user_id IS NOT NULL))
 AND ((m.role='Owner' AND r.delegation_revision IS NULL AND (r.mode='ask' OR r.configured_by=r.approver))
 OR (a.owner_id=r.configured_by AND r.delegation_revision=d.revision AND dm.user_id IS NOT NULL
 AND (r.approver=d.approver OR (r.mode='ask' AND g.granted_by=d.approver AND g.enabled AND g.revision=r.approver_revision)) AND (d.ceiling='allow' OR (d.ceiling='ask' AND r.mode='ask'))));
