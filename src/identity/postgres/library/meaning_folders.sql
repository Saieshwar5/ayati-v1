-- Schema 156: meaning search is off until a folder's manager turns it on. Only what is in
-- such a folder is sent to the embedding and summary models; everything else is found by
-- words. An agent may propose a folder; a person decides.
ALTER TABLE relay_library_items ADD COLUMN meaning BOOLEAN NOT NULL DEFAULT false;
-- Whether an item is in a folder (at any depth) with meaning search on.
CREATE FUNCTION relay_library_meaning(TEXT, TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM relay_library_items f
  WHERE f.workspace=$1 AND f.id = ANY(relay_library_path($1,$2)) AND f.kind='folder' AND f.meaning)
$$;
CREATE TABLE relay_library_meaning_proposals (
 workspace TEXT NOT NULL, folder TEXT NOT NULL, agent TEXT NOT NULL,
 reason TEXT NOT NULL CHECK(char_length(reason) BETWEEN 1 AND 300), at BIGINT NOT NULL,
 PRIMARY KEY(workspace,folder,agent),
 FOREIGN KEY(workspace,folder) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
-- No folder has it on yet, so nothing stays embedded.
DO $$ BEGIN
 IF to_regclass('relay_library_embeddings') IS NOT NULL THEN
  EXECUTE 'DELETE FROM relay_library_embeddings';
 END IF;
END $$;
