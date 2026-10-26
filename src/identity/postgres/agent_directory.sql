SELECT to_jsonb(a) || jsonb_build_object(
 'creator_name',COALESCE(u.display_name,u.email,'Unknown'),
 'owner_name',COALESCE(o.display_name,o.email,'Former member'),
 'editable',(a.owner_id=$2 OR (a.scope='workspace' AND $3)),
 'pending',(
  SELECT count(*) FROM relay_chat_jobs j
  JOIN relay_chats c ON c.workspace=j.workspace AND c.id=j.payload->>'chat'
  WHERE j.workspace=a.workspace AND j.payload->>'agent'=a.id
  AND j.state IN ('queued','running','waiting')
  AND (c.visibility='public' OR EXISTS(
   SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))
  AND (j.payload->>'requester'=$2 OR NOT EXISTS(
   SELECT 1 FROM relay_library_sources s WHERE s.workspace=j.workspace AND s.reply=j.reply))
  AND NOT EXISTS(
   SELECT 1 FROM relay_library_sources s
   LEFT JOIN relay_library_items i ON i.workspace=s.workspace AND i.id=s.item
   WHERE s.workspace=j.workspace AND s.reply=j.reply
   AND (i.id IS NULL OR i.trashed OR i.access_revision<>s.access_revision
    OR NOT EXISTS(SELECT 1 FROM relay_library_agents g WHERE g.workspace=s.workspace AND g.item=s.item AND g.agent=a.id)
    OR c.visibility<>'private'
    OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id<>$2)))
 )) AS data
FROM relay_shared_agents a
LEFT JOIN relay_users u ON u.id=a.creator_id
LEFT JOIN relay_users o ON o.id=a.owner_id
WHERE a.workspace=$1 AND (a.owner_id=$2 OR (a.scope='workspace' AND ($3 OR EXISTS(
 SELECT 1 FROM relay_chat_access g WHERE g.workspace=a.workspace AND g.user_id=$2 AND g.agent=a.id))))
ORDER BY a.created_at,a.id LIMIT 500
