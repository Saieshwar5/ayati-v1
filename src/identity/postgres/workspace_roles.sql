-- Existing Admins become Users; ownership is never granted implicitly.
ALTER TABLE relay_members DROP CONSTRAINT relay_members_role_check;
ALTER TABLE relay_invitations DROP CONSTRAINT relay_invitations_role_check;
DROP INDEX relay_one_owner;
INSERT INTO relay_audit(actor,workspace,action,target,at)
SELECT user_id,workspace,'legacy_role_migrated',role || ' -> User',extract(epoch FROM now())::bigint
FROM relay_members WHERE role IN ('Admin','Member');
UPDATE relay_members SET role='User' WHERE role IN ('Admin','Member');
UPDATE relay_invitations SET role='User' WHERE role IN ('Admin','Member');
ALTER TABLE relay_members ADD CONSTRAINT relay_members_role_check CHECK(role IN ('Owner','User'));
ALTER TABLE relay_invitations ADD CONSTRAINT relay_invitations_role_check CHECK(role='User');
