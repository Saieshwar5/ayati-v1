-- Schema 153: folders inside folders, and admins who can open everything.
-- An item follows the sharing of the nearest folder above it that has its own: a
-- top-level folder, or a subfolder marked `shares`. Admins get edit access to every
-- item; relay_library_granted is what a person was given without being an admin.
ALTER TABLE relay_library_items DROP CONSTRAINT relay_library_items_folder_shape;
ALTER TABLE relay_library_items ADD CONSTRAINT relay_library_items_folder_shape
 CHECK(folder IS NULL OR folder<>id);
ALTER TABLE relay_library_items ADD COLUMN shares BOOLEAN NOT NULL DEFAULT false;

-- The item and every folder above it, nearest first.
CREATE FUNCTION relay_library_path(TEXT, TEXT) RETURNS TEXT[] LANGUAGE sql STABLE AS $$
 WITH RECURSIVE up(id, folder, depth) AS (
  SELECT id, folder, 0 FROM relay_library_items WHERE workspace=$1 AND id=$2
  UNION ALL
  SELECT f.id, f.folder, up.depth+1 FROM up
  JOIN relay_library_items f ON f.workspace=$1 AND f.id=up.folder WHERE up.depth<8)
 SELECT COALESCE(array_agg(id ORDER BY depth), '{}') FROM up
$$;
-- The item whose sharing this one follows: itself when it is unfiled or a folder with
-- its own sharing, else the nearest such folder above it.
CREATE FUNCTION relay_library_source(TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 WITH RECURSIVE up(id, folder, own, depth) AS (
  SELECT id, folder, kind='folder' AND shares, 0 FROM relay_library_items WHERE workspace=$1 AND id=$2
  UNION ALL
  SELECT f.id, f.folder, f.shares, up.depth+1 FROM up
  JOIN relay_library_items f ON f.workspace=$1 AND f.id=up.folder
  WHERE up.depth<8 AND NOT up.own)
 SELECT id FROM up WHERE own OR folder IS NULL ORDER BY depth LIMIT 1
$$;
-- Everything inside a folder, at any depth.
CREATE FUNCTION relay_library_inside(TEXT, TEXT) RETURNS SETOF TEXT LANGUAGE sql STABLE AS $$
 WITH RECURSIVE down(id, depth) AS (
  SELECT id, 1 FROM relay_library_items WHERE workspace=$1 AND folder=$2
  UNION ALL
  SELECT c.id, down.depth+1 FROM down
  JOIN relay_library_items c ON c.workspace=$1 AND c.folder=down.id WHERE down.depth<8)
 SELECT id FROM down
$$;
CREATE FUNCTION relay_library_admin(TEXT, TEXT) RETURNS BOOLEAN LANGUAGE sql STABLE AS $$
 SELECT EXISTS(SELECT 1 FROM relay_members WHERE workspace=$1 AND user_id=$2 AND role='Admin')
$$;
-- The keeping rule an item is under: its nearest folder that has one.
CREATE FUNCTION relay_library_keep_days(TEXT, TEXT) RETURNS INTEGER LANGUAGE sql STABLE AS $$
 SELECT k.keep_days FROM unnest(relay_library_path($1,$2)) WITH ORDINALITY AS p(id, n)
 JOIN relay_library_keeping k ON k.workspace=$1 AND k.folder=p.id
 WHERE p.id<>$2 ORDER BY p.n LIMIT 1
$$;

ALTER FUNCTION relay_library_access(TEXT, TEXT, TEXT) RENAME TO relay_library_granted;
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
-- A person's access to an item: 'edit', 'view' or NULL. Admins can open everything.
CREATE FUNCTION relay_library_access(TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT CASE WHEN relay_library_admin($1,$3)
   AND EXISTS(SELECT 1 FROM relay_library_items WHERE workspace=$1 AND id=$2) THEN 'edit'
  ELSE relay_library_granted($1,$2,$3) END
$$;

CREATE OR REPLACE FUNCTION relay_library_agent_use(TEXT, TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT g.use_for
 FROM relay_chat_jobs j
 JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
 JOIN relay_library_items i ON i.workspace=j.workspace AND i.id=$2 AND NOT i.trashed
 JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=relay_library_source(i.workspace,i.id) AND NOT s.trashed
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
      WHERE c.workspace=$1 AND c.id=m.chat
       AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people cp
        WHERE cp.workspace=$1 AND cp.chat=c.id AND cp.user_id=p.user_id))))))
$$;
