WITH candidates AS (
 SELECT 'question'::text AS kind,q.id,q.reply,'pending'::text AS version,
  q.created_ms AS at,left(q.specification->>'title',180) AS title,'pending'::text AS status
 FROM relay_task_questions q JOIN relay_chat_jobs j ON j.workspace=q.workspace AND j.reply=q.reply
 WHERE q.workspace=$1 AND q.state='pending' AND j.state<>'finished' AND j.payload->>'requester'=$2
 UNION ALL
 SELECT 'approval',a.id,a.job,
  CASE WHEN a.state IN ('executing','uncertain') THEN 'uncertain' ELSE 'pending' END,
  a.updated*1000,left(a.title,180),
  CASE WHEN a.state IN ('executing','uncertain') THEN 'uncertain' ELSE 'pending' END
 FROM relay_control_actions a WHERE a.workspace=$1 AND a.approver=$2 AND
  ((a.state='pending' AND a.requires_approval) OR a.state='uncertain'
   OR (a.state='executing' AND a.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60))
 UNION ALL
 SELECT 'outcome',j.reply,j.reply,m.state||':'||COALESCE(j.finished_ms,m.at*1000)::text,
  COALESCE(j.finished_ms,m.at*1000),COALESCE(left(request.content,180),'Agent request'),m.state
 FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
 LEFT JOIN LATERAL (
  SELECT r.content FROM relay_chat_messages r WHERE r.workspace=j.workspace AND r.chat=m.chat
   AND r.kind='user' AND r.author=j.payload->>'requester' AND r.target=j.payload->>'agent'
   AND r.thread IS NOT DISTINCT FROM m.thread AND r.sequence<m.sequence
  ORDER BY r.sequence DESC LIMIT 1
 ) request ON TRUE
 WHERE j.workspace=$1 AND j.state='finished' AND j.payload->>'requester'=$2
  AND m.state IN ('complete','failed','interrupted')
), visible AS (
 SELECT n.*,j.payload,m.chat,m.thread,m.sequence,c.name AS conversation_name,c.archived,
  g.name AS agent_name,g.id AS agent,r.read_ms,r.reviewed_ms
 FROM candidates n
 JOIN relay_chat_jobs j ON j.workspace=$1 AND j.reply=n.reply
 JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
 JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
 JOIN relay_shared_agents g ON g.workspace=j.workspace AND g.id=j.payload->>'agent'
 LEFT JOIN relay_attention_receipts r ON r.workspace=$1 AND r.user_id=$2
  AND r.kind=n.kind AND r.source=n.id AND r.version=n.version
 WHERE (g.scope<>'private' OR g.owner_id=$2)
 AND (c.visibility='public' OR EXISTS(
  SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))
 -- Mirror the Library boundary before LIMIT. Rust rechecks the authoritative
 -- source function before returning any title, count, cursor or acknowledgment.
 AND NOT EXISTS (
  SELECT 1 FROM relay_library_sources s
  LEFT JOIN relay_library_items i ON i.workspace=s.workspace AND i.id=s.item
  WHERE s.workspace=j.workspace AND s.reply=j.reply AND (
   i.id IS NULL OR i.trashed OR i.access_revision<>s.access_revision
   OR NOT(i.steward=$2 OR i.visibility='workspace' OR (i.visibility='selected' AND EXISTS(
    SELECT 1 FROM relay_library_people p WHERE p.workspace=i.workspace AND p.item=i.id AND p.user_id=$2)))
   OR NOT EXISTS(SELECT 1 FROM relay_library_agents a WHERE a.workspace=i.workspace AND a.item=i.id AND a.agent=g.id)
   OR c.visibility<>'private'
   OR NOT EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2)
   OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id<>$2)
  )
 )
)
SELECT * FROM visible
WHERE ($3='all' OR ($3='recent' AND kind='outcome') OR ($3='needs_you' AND
 (kind<>'outcome' OR (status<>'complete' AND reviewed_ms IS NULL))))
AND (at,kind,id)<($4,$5,$6)
AND ($7::text IS NULL OR (kind=$7 AND id=$8))
ORDER BY at DESC,kind DESC,id DESC LIMIT 26
