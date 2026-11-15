-- Schema 122: agents have one or more equal owners. The manager role is gone.
-- Every owner changes the agent, decides its approvals, sees its work and
-- adds or removes owners; admins have the same authority over every agent.
-- An agent whose last owner left has none and stays paused until an admin
-- gives it one.
--
-- The former manager lists are kept as a record, no longer read.
ALTER TABLE relay_agent_managers RENAME TO relay_agent_former_managers;
-- An agent that was running when its last owner left resumes once it has one.
ALTER TABLE relay_shared_agents ADD COLUMN resume_with_owner BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE relay_agent_owners (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, user_id TEXT NOT NULL,
 added_by TEXT REFERENCES relay_users(id),
 at BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,agent,user_id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 -- Someone who leaves the workspace owns nothing in it.
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_agent_owners_person ON relay_agent_owners(workspace,user_id);

CREATE FUNCTION relay_agent_owns(w TEXT, agent_id TEXT, person TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM relay_agent_owners o WHERE o.workspace=w AND o.agent=agent_id AND o.user_id=person)
$$;

-- relay_shared_agents.owner_id stays, kept in step: the first owner still in
-- the list, or NULL when there is none. Views and earlier code that only ask
-- "does it have an owner" keep working; who may act is read from the list.
CREATE FUNCTION relay_agent_first_owner() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE w TEXT; agent_id TEXT; first TEXT;
BEGIN
 IF TG_OP='DELETE' THEN w:=OLD.workspace; agent_id:=OLD.agent; ELSE w:=NEW.workspace; agent_id:=NEW.agent; END IF;
 SELECT o.user_id INTO first FROM relay_agent_owners o WHERE o.workspace=w AND o.agent=agent_id ORDER BY o.at,o.user_id LIMIT 1;
 UPDATE relay_shared_agents SET owner_id=first WHERE workspace=w AND id=agent_id AND owner_id IS DISTINCT FROM first;
 RETURN NULL;
END $$;
CREATE TRIGGER relay_agent_owners_first AFTER INSERT OR DELETE ON relay_agent_owners
 FOR EACH ROW EXECUTE FUNCTION relay_agent_first_owner();

-- A new agent's owner_id names its first owner.
CREATE FUNCTION relay_agent_created_owner() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 INSERT INTO relay_agent_owners(workspace,agent,user_id,added_by)
  SELECT NEW.workspace,NEW.id,NEW.owner_id,NEW.owner_id FROM relay_members m
  WHERE m.workspace=NEW.workspace AND m.user_id=NEW.owner_id
  ON CONFLICT DO NOTHING;
 RETURN NULL;
END $$;
CREATE TRIGGER relay_agent_created_owner AFTER INSERT ON relay_shared_agents
 FOR EACH ROW WHEN (NEW.owner_id IS NOT NULL) EXECUTE FUNCTION relay_agent_created_owner();

-- Today's owners become the first owners. An owner_id naming someone no longer
-- in the workspace leaves the agent without one.
INSERT INTO relay_agent_owners(workspace,agent,user_id,added_by)
 SELECT a.workspace,a.id,a.owner_id,a.owner_id FROM relay_shared_agents a
 JOIN relay_members m ON m.workspace=a.workspace AND m.user_id=a.owner_id;
UPDATE relay_shared_agents a SET owner_id=NULL WHERE owner_id IS NOT NULL
 AND NOT EXISTS(SELECT 1 FROM relay_agent_owners o WHERE o.workspace=a.workspace AND o.agent=a.id);

-- A managed agent without an owner is paused, to resume once it has one.
UPDATE relay_shared_agents
 SET resume_with_owner=(audience<>'only_me' AND COALESCE(settings->>'paused','false')<>'true'),
     settings=jsonb_set(settings,'{paused}','true')
 WHERE owner_id IS NULL AND revision>0;

-- "Only me" now means only its owners.
CREATE OR REPLACE FUNCTION relay_agent_can_talk(w TEXT, agent_id TEXT, person TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT EXISTS(
  SELECT 1 FROM relay_shared_agents a
  JOIN relay_members m ON m.workspace=a.workspace AND m.user_id=person
  WHERE a.workspace=w AND a.id=agent_id AND (
   relay_agent_owns(w,agent_id,person)
   OR a.audience='everyone'
   OR (a.audience='selected' AND (m.role='Admin' OR EXISTS(
    SELECT 1 FROM relay_chat_access g WHERE g.workspace=w AND g.agent=agent_id AND g.user_id=person)))))
$$;
