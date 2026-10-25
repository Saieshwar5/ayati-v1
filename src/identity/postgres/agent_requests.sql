SELECT j.payload,j.sequence,j.state AS job_state,j.lease_until,j.started_ms,j.finished_ms,
 COALESCE(j.created_ms,m.at*1000) AS created_ms,m.id,m.chat,m.thread,m.state,m.content,m.progress,
 c.name AS conversation_name,COALESCE(u.display_name,u.email) AS requester_name,
 source.id AS source_message,left(source.content,180) AS summary,
 a.state AS attempt_state
FROM relay_chat_jobs j
JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
LEFT JOIN relay_users u ON u.id=j.payload->>'requester'
LEFT JOIN relay_worker_assignments a ON a.id=j.claim
-- Intake saves the requesting message and its reply in one workspace transaction.
-- A summary must not accidentally contain the entire assembled model context.
LEFT JOIN LATERAL (
 SELECT r.id,r.content FROM relay_chat_messages r
 WHERE r.workspace=m.workspace AND r.chat=m.chat AND r.sequence<m.sequence
 AND r.kind='user' AND r.author=j.payload->>'requester' AND r.target=j.payload->>'agent'
 AND r.thread IS NOT DISTINCT FROM m.thread
 ORDER BY r.sequence DESC LIMIT 1
) source ON TRUE
WHERE j.workspace=$1 AND j.payload->>'agent'=$2 AND j.sequence<$6
AND (NOT $4 OR j.payload->>'requester'=$3)
AND (NOT $5 OR j.state<>'finished')
AND ($7::text IS NULL OR j.reply=$7)
AND (c.visibility='public' OR EXISTS(
 SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3))
AND (j.payload->>'requester'=$3 OR NOT EXISTS(
 SELECT 1 FROM relay_library_sources s WHERE s.workspace=j.workspace AND s.reply=j.reply))
ORDER BY j.sequence DESC LIMIT 26
