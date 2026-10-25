CREATE TABLE relay_control_systems (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), server TEXT NOT NULL,
 approved BOOLEAN NOT NULL, revision BIGINT NOT NULL,
 PRIMARY KEY(workspace,server)
);
INSERT INTO relay_control_systems(workspace,server,approved,revision)
 SELECT workspace,server,true,1 FROM relay_control_connections;
CREATE TABLE relay_agent_creators (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL, enabled BOOLEAN NOT NULL,
 revision BIGINT NOT NULL, PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_control_delegations (
 workspace TEXT NOT NULL, connection TEXT NOT NULL, user_id TEXT NOT NULL,
 operation TEXT NOT NULL, ceiling TEXT NOT NULL CHECK(ceiling IN ('allow','ask','block')),
 approver TEXT NOT NULL REFERENCES relay_users(id), revision BIGINT NOT NULL,
 PRIMARY KEY(workspace,connection,user_id,operation),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
ALTER TABLE relay_control_rules ADD COLUMN configured_by TEXT REFERENCES relay_users(id);
UPDATE relay_control_rules SET configured_by=approver;
ALTER TABLE relay_control_rules ALTER COLUMN configured_by SET NOT NULL;
ALTER TABLE relay_control_rules ADD COLUMN delegation_revision BIGINT;

-- Runtime reads this view at discovery, admission, claim and final dispatch.
-- Administrative roles and delegations are live facts, not cached prompt claims.
CREATE VIEW relay_control_effective_rules AS
 SELECT r.* FROM relay_control_rules r
 JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection
 JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server
 JOIN relay_shared_agents a ON a.workspace=r.workspace AND a.id=r.agent
 JOIN relay_members m ON m.workspace=r.workspace AND m.user_id=r.configured_by
 JOIN relay_members p ON p.workspace=r.workspace AND p.user_id=r.approver AND p.role='Owner'
 LEFT JOIN relay_control_delegations d ON d.workspace=r.workspace AND d.connection=r.connection
  AND d.user_id=r.configured_by AND d.operation=r.operation
 WHERE s.approved AND c.enabled AND r.mode<>'block' AND (
  (m.role='Owner' AND r.delegation_revision IS NULL AND r.configured_by=r.approver)
  OR (a.owner_id=r.configured_by AND r.delegation_revision=d.revision
      AND r.approver=d.approver AND (d.ceiling='allow' OR (d.ceiling='ask' AND r.mode='ask')))
 );
