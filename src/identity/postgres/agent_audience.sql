-- One agent type (D231 slice 2, D233): "who can talk to it" replaces private and
-- workspace agents, everyone may create agents, and each agent has managers.
--   everyone: every member of the workspace;
--   selected: the owner, admins and the people listed in relay_chat_access
--             (exactly what a workspace agent allowed before);
--   only_me:  the owner alone (what a private agent allowed before).
-- In a channel or group the agent belongs to, anyone who can see that conversation
-- may also ask a shared agent there; the server checks that per request.
ALTER TABLE relay_shared_agents ADD COLUMN audience TEXT NOT NULL DEFAULT 'selected'
 CHECK(audience IN ('everyone','selected','only_me'));
UPDATE relay_shared_agents SET audience='only_me' WHERE scope='private';
ALTER TABLE relay_shared_agents DROP COLUMN scope;

-- Managers decide "Ask first" requests, see the agent's activity and can pause it
-- (approvals start using this list in M3). The owner is the first manager.
CREATE TABLE relay_agent_managers (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, user_id TEXT NOT NULL,
 added_by TEXT NOT NULL REFERENCES relay_users(id), at BIGINT NOT NULL,
 PRIMARY KEY(workspace,agent,user_id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
INSERT INTO relay_agent_managers(workspace,agent,user_id,added_by,at)
SELECT a.workspace,a.id,a.owner_id,a.owner_id,extract(epoch FROM now())::bigint
FROM relay_shared_agents a
JOIN relay_members m ON m.workspace=a.workspace AND m.user_id=a.owner_id;

-- Everyone may create agents, so the per-person creator right goes away.
DROP TABLE relay_agent_creators;

-- The single definition of who may talk to an agent outside a channel. Pausing is
-- checked separately, so a paused agent still shows who it is for.
CREATE FUNCTION relay_agent_can_talk(w TEXT, agent_id TEXT, person TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT EXISTS(
  SELECT 1 FROM relay_shared_agents a
  JOIN relay_members m ON m.workspace=a.workspace AND m.user_id=person
  WHERE a.workspace=w AND a.id=agent_id AND (
   a.owner_id=person
   OR a.audience='everyone'
   OR (a.audience='selected' AND (m.role='Admin' OR EXISTS(
    SELECT 1 FROM relay_chat_access g WHERE g.workspace=w AND g.agent=agent_id AND g.user_id=person)))))
$$;

-- Same conditions as schema 43, with "who can talk" in place of scope and grants.
CREATE OR REPLACE FUNCTION relay_communication_current(w TEXT, c TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM relay_communication_conversations WHERE workspace=w AND chat=c)
 OR EXISTS(
    SELECT 1 FROM relay_communication_conversations v
    JOIN relay_communication_endpoints e ON e.workspace=v.workspace AND e.id=v.endpoint
    JOIN relay_communication_bindings b ON b.workspace=v.workspace AND b.endpoint=v.endpoint AND b.sender=v.sender
    JOIN relay_shared_agents a ON a.workspace=v.workspace AND a.id=e.agent
    JOIN relay_chats ch ON ch.workspace=v.workspace AND ch.id=v.chat
    WHERE v.workspace=w AND v.chat=c AND e.enabled AND b.enabled
      AND e.revision=v.endpoint_revision AND b.revision=v.binding_revision
      AND ch.visibility='private' AND NOT ch.archived
      AND NOT COALESCE((a.settings->>'paused')::BOOLEAN,FALSE)
      AND relay_agent_can_talk(w,e.agent,b.user_id)
      AND (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=w AND p.chat=c)=1
      AND EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=w AND p.chat=c AND p.user_id=b.user_id)
      AND (SELECT count(*) FROM relay_chat_agents x WHERE x.workspace=w AND x.chat=c)=1
      AND EXISTS(SELECT 1 FROM relay_chat_agents x WHERE x.workspace=w AND x.chat=c AND x.agent=e.agent)
 )
$$;
