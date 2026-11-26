-- Schema 155: each agent's own folder. It holds one subfolder per conversation, where
-- what the agent makes for that conversation is kept. Access follows the conversation:
-- whoever is in it (everyone, for a public channel) can open and add to its subfolder,
-- and the agent's owners look after everything the agent keeps. `chat` is '' for the
-- agent's top folder.
CREATE TABLE relay_library_agent_folders (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, chat TEXT NOT NULL DEFAULT '', folder TEXT NOT NULL,
 PRIMARY KEY(workspace,agent,chat), UNIQUE(workspace,folder),
 FOREIGN KEY(workspace,folder) REFERENCES relay_library_items(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE OR REPLACE FUNCTION relay_library_granted(TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT CASE
  WHEN i.steward=$3 OR s.steward=$3 THEN 'edit'
  WHEN EXISTS(SELECT 1 FROM relay_library_people g
   WHERE g.workspace=$1 AND g.item=s.id AND g.user_id=$3 AND g.access='edit') THEN 'edit'
  WHEN EXISTS(SELECT 1 FROM relay_library_agent_folders af
   WHERE af.workspace=$1 AND af.folder=s.id
    AND EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=$1 AND m.user_id=$3)
    AND (EXISTS(SELECT 1 FROM relay_agent_owners o
      WHERE o.workspace=$1 AND o.agent=af.agent AND o.user_id=$3)
     OR EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=$1 AND c.id=af.chat
      AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people cp
       WHERE cp.workspace=$1 AND cp.chat=c.id AND cp.user_id=$3))))) THEN 'edit'
  WHEN s.visibility='workspace' OR EXISTS(SELECT 1 FROM relay_library_people g
   WHERE g.workspace=$1 AND g.item=s.id AND g.user_id=$3) THEN 'view'
 END
 FROM relay_library_items i
 JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=relay_library_source(i.workspace,i.id)
 WHERE i.workspace=$1 AND i.id=$2
$$;
