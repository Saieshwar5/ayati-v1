SELECT j.payload,j.sequence,j.state AS job_state,j.lease_until,j.started_ms,j.finished_ms,
 j.created_ms AS created_ms,m.id,m.chat,m.thread,m.state,m.content,m.progress,
 c.name AS conversation_name,c.kind AS conversation_kind,
 (SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=j.payload->>'requester') AS requester_name,
 (SELECT s.id FROM (/*SOURCE*/) s) AS source_message,(SELECT left(s.content,180) FROM (/*SOURCE*/) s) AS summary,
 (SELECT x.state FROM relay_worker_assignments x WHERE x.id=j.claim) AS attempt_state,
 ag.id AS agent,ag.name AS agent_name,ag.settings->>'avatar' AS agent_avatar,k.kind,
 (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2)) AS in_conversation
/*FILTER*/
AND j.sequence<$12
AND ($14::text IS NULL OR j.reply=$14)
ORDER BY j.sequence DESC LIMIT 26
