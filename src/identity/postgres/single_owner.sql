-- Schema 122: one owner per agent. The manager role is removed: the owner
-- decides the agent's approvals, sees its work and pauses it, and admins have
-- the same authority over every agent. An agent whose owner left has no owner
-- (owner_id NULL) and stays paused until an admin gives it one.
--
-- The former manager lists are kept as a record, no longer read by the product.
ALTER TABLE relay_agent_managers RENAME TO relay_agent_former_managers;
-- An agent that was running when its owner left resumes once it has an owner.
ALTER TABLE relay_shared_agents ADD COLUMN resume_with_owner BOOLEAN NOT NULL DEFAULT false;
-- A managed agent that already had no owner follows the same rule from now on.
UPDATE relay_shared_agents
 SET resume_with_owner=(audience<>'only_me' AND COALESCE(settings->>'paused','false')<>'true'),
     settings=jsonb_set(settings,'{paused}','true')
 WHERE owner_id IS NULL AND revision>0;
