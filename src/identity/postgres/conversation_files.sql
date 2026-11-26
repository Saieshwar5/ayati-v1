-- Schema 159: files belong to their conversation, not to the Library. Agents' own
-- folders (schema 155) are gone: what people attach and what an agent makes for a
-- conversation are that conversation's files, seen by the people in it. `made_by` names
-- the agent that made a file; `library_item` the Library file a person saved it as.
ALTER TABLE relay_objects ADD COLUMN made_by TEXT, ADD COLUMN library_item TEXT;
CREATE INDEX relay_objects_made_by ON relay_objects(workspace,made_by) WHERE made_by IS NOT NULL;
CREATE OR REPLACE FUNCTION relay_library_granted(TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT CASE
  WHEN i.steward=$3 OR s.steward=$3 THEN 'edit'
  WHEN EXISTS(SELECT 1 FROM relay_library_people g
   WHERE g.workspace=$1 AND g.item=s.id AND g.user_id=$3 AND g.access='edit') THEN 'edit'
  WHEN s.visibility='workspace' OR EXISTS(SELECT 1 FROM relay_library_people g
   WHERE g.workspace=$1 AND g.item=s.id AND g.user_id=$3) THEN 'view'
 END
 FROM relay_library_items i
 JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=relay_library_source(i.workspace,i.id)
 WHERE i.workspace=$1 AND i.id=$2
$$;
DELETE FROM relay_library_keeping k USING relay_library_agent_folders f
 WHERE k.workspace=f.workspace AND k.folder=f.folder;
DROP TABLE relay_library_agent_folders;
