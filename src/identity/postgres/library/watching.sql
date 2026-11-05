-- Schema 83 (LB6): watched folders. A folder's manager asks an agent to handle what
-- arrives in it; each new item becomes one piece of agent work in the watcher's own
-- conversation, for that person. Agents may record key fields on an item.
CREATE TABLE relay_library_watchers (
 workspace TEXT NOT NULL, folder TEXT NOT NULL, agent TEXT NOT NULL,
 principal TEXT NOT NULL REFERENCES relay_users(id), chat TEXT NOT NULL,
 instructions TEXT NOT NULL CHECK(char_length(instructions) BETWEEN 1 AND 2000),
 set_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,folder),
 FOREIGN KEY(workspace,folder) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_library_arrivals (
 workspace TEXT NOT NULL, item TEXT NOT NULL, folder TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','started','skipped')),
 reply TEXT, detail TEXT NOT NULL DEFAULT '', at BIGINT NOT NULL,
 PRIMARY KEY(workspace,item,folder),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_library_arrivals_waiting ON relay_library_arrivals(at) WHERE state='pending';
CREATE TABLE relay_library_fields (
 workspace TEXT NOT NULL, item TEXT NOT NULL,
 fields JSONB NOT NULL CHECK(jsonb_typeof(fields)='object' AND octet_length(fields::text)<=4096),
 agent TEXT, set_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,item),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
-- Whatever way an item lands in a watched folder (created there, uploaded and filed,
-- or moved in), it is queued once for the watcher.
CREATE FUNCTION relay_library_arrival() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.folder IS NOT NULL AND NEW.kind <> 'folder' AND NOT NEW.trashed
    AND (TG_OP = 'INSERT' OR OLD.folder IS DISTINCT FROM NEW.folder)
    AND EXISTS(SELECT 1 FROM relay_library_watchers w WHERE w.workspace=NEW.workspace AND w.folder=NEW.folder) THEN
  INSERT INTO relay_library_arrivals(workspace,item,folder,at)
  VALUES(NEW.workspace,NEW.id,NEW.folder,floor(extract(epoch FROM clock_timestamp()))::bigint)
  ON CONFLICT DO NOTHING;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_library_arrival AFTER INSERT OR UPDATE OF folder ON relay_library_items
 FOR EACH ROW EXECUTE FUNCTION relay_library_arrival();
