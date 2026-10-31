FROM relay_chat_jobs j
JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
JOIN relay_shared_agents ag ON ag.workspace=j.workspace AND ag.id=j.payload->>'agent'
CROSS JOIN LATERAL (SELECT CASE
 WHEN EXISTS(SELECT 1 FROM relay_schedule_occurrences o WHERE o.workspace=j.workspace AND o.reply=j.reply) THEN 'schedule'
 WHEN EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=j.workspace AND d.reply=j.reply) THEN 'trigger'
 WHEN EXISTS(SELECT 1 FROM relay_agent_handoffs h WHERE h.workspace=j.workspace AND h.child=j.reply) THEN 'handoff'
 ELSE 'chat' END AS kind) k
WHERE j.workspace=$1
-- Who sees it: the requester, anyone who can see the conversation, the agent's
-- managers and admins ($3).
AND (j.payload->>'requester'=$2 OR c.visibility='public' OR $3
 OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2)
 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=j.workspace AND g.agent=ag.id AND g.user_id=$2))
-- Work that used Library sources stays with its requester.
AND (j.payload->>'requester'=$2 OR NOT EXISTS(
 SELECT 1 FROM relay_library_sources s WHERE s.workspace=j.workspace AND s.reply=j.reply))
AND ($4::text IS NULL OR ag.id=$4)
AND ($5::text IS NULL OR j.payload->>'requester'=$5)
AND ($6::text IS NULL OR m.chat=$6)
AND j.created_ms>=$8 AND j.created_ms<$9
AND ($10::text IS NULL OR m.content ILIKE $10 OR c.name ILIKE $10 OR ag.name ILIKE $10
 OR EXISTS(SELECT 1 FROM (/*SOURCE*/) s WHERE s.content ILIKE $10))
AND ($11::text IS NULL OR k.kind=$11)
AND ($7::text IS NULL
 OR ($7='active' AND j.state<>'finished')
 OR ($7='waiting' AND j.state='waiting')
 OR ($7='done' AND j.state='finished' AND m.state='complete')
 OR ($7='failed' AND m.state IN ('failed','interrupted'))
 OR ($7='stopped' AND m.state='cancelled')
 OR ($7='review' AND (m.state IN ('failed','interrupted') OR EXISTS(
  SELECT 1 FROM relay_control_actions x WHERE x.workspace=j.workspace AND x.job=j.reply
  AND (x.state='uncertain' OR (x.state='executing' AND x.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60))))))
