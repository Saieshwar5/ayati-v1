-- Schema 69 (Library LB1): folders whose sharing their items follow, per-agent access
-- with a "use for" audience, and receipts of what agents read while answering.
ALTER TABLE relay_library_items DROP CONSTRAINT relay_library_items_kind_check;
ALTER TABLE relay_library_items ADD CONSTRAINT relay_library_items_kind_check
 CHECK(kind IN ('page','file','folder'));
ALTER TABLE relay_library_items ADD COLUMN folder TEXT;
ALTER TABLE relay_library_items ADD CONSTRAINT relay_library_items_in_folder
 FOREIGN KEY(workspace,folder) REFERENCES relay_library_items(workspace,id);
-- One level: folders hold pages and files, not other folders.
ALTER TABLE relay_library_items ADD CONSTRAINT relay_library_items_folder_shape
 CHECK(folder IS NULL OR (kind<>'folder' AND folder<>id));
CREATE INDEX relay_library_in_folder ON relay_library_items(workspace,folder) WHERE folder IS NOT NULL;

-- 'read' is the only level granted until agent editing (LB3); the others are reserved.
ALTER TABLE relay_library_agents
 ADD COLUMN level TEXT NOT NULL DEFAULT 'read'
  CHECK(level IN ('read','suggest','edit_approval','edit','save')),
 ADD COLUMN use_for TEXT NOT NULL DEFAULT 'viewers'
  CHECK(use_for IN ('viewers','staff','customers'));

CREATE TABLE relay_library_reads (
 workspace TEXT NOT NULL, reply TEXT NOT NULL, item TEXT NOT NULL,
 revision BIGINT NOT NULL, access_revision BIGINT NOT NULL, title TEXT NOT NULL, at BIGINT NOT NULL,
 PRIMARY KEY(workspace,reply,item),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id)
);

-- A person's access to an item: 'edit', 'view' or NULL. An item in a folder follows the
-- folder's sharing; its own steward keeps access.
CREATE FUNCTION relay_library_access(TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT CASE
  WHEN i.steward=$3 OR s.steward=$3 THEN 'edit'
  WHEN EXISTS(SELECT 1 FROM relay_library_people g
   WHERE g.workspace=$1 AND g.item=s.id AND g.user_id=$3 AND g.access='edit') THEN 'edit'
  WHEN s.visibility='workspace' OR EXISTS(SELECT 1 FROM relay_library_people g
   WHERE g.workspace=$1 AND g.item=s.id AND g.user_id=$3) THEN 'view'
 END
 FROM relay_library_items i
 JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id)
 WHERE i.workspace=$1 AND i.id=$2
$$;

-- Whether an agent may use an item for one reply, given everyone who will see that reply:
-- the reply's conversation (every member when it is public) and a schedule's posting
-- conversation. Returns the grant's audience, or NULL. Replies going outside the
-- workspace may use only customer documents, which are not offered yet.
CREATE FUNCTION relay_library_agent_use(TEXT, TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT g.use_for
 FROM relay_chat_jobs j
 JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
 JOIN relay_library_items i ON i.workspace=j.workspace AND i.id=$2 AND NOT i.trashed
 JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id) AND NOT s.trashed
 JOIN relay_library_agents g ON g.workspace=s.workspace AND g.item=s.id AND g.agent=$3
 WHERE j.workspace=$1 AND j.reply=$4
  AND NOT EXISTS(SELECT 1 FROM relay_communication_inbox x
   WHERE x.workspace=$1 AND x.reply=$4 AND x.transport<>'chat')
  AND (g.use_for='staff' OR (g.use_for='viewers'
   AND relay_library_access($1,$2,j.payload->>'requester') IS NOT NULL
   AND NOT EXISTS(
    SELECT 1 FROM relay_members p
    WHERE p.workspace=$1 AND relay_library_access($1,$2,p.user_id) IS NULL
     AND EXISTS(SELECT 1 FROM relay_chats c
      WHERE c.workspace=$1
       AND (c.id=m.chat OR c.id IN (SELECT sc.chat FROM relay_schedule_occurrences o
        JOIN relay_agent_schedules sc ON sc.workspace=o.workspace AND sc.id=o.schedule
        WHERE o.workspace=$1 AND o.reply=$4))
       AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people cp
        WHERE cp.workspace=$1 AND cp.chat=c.id AND cp.user_id=p.user_id))))))
$$;
