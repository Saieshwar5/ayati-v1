-- Appended to pool_load.sql. Class is immutable controller intake metadata.
-- Connection membership is a bounded live hint, never authority.
SELECT j.reply,j.workspace,j.attempts,j.generation,j.work_class FROM relay_chat_jobs j
JOIN relay_shared_agents a ON a.workspace=j.workspace AND a.id=j.payload->>'agent'
LEFT JOIN workspace_load w ON w.workspace=j.workspace
-- Five attempts (attempt_resume::MAX_ATTEMPTS); each continues from the last.
WHERE j.dispatch='pooled' AND j.attempts<5
AND (j.state='queued' OR (j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint))
-- A gateway executor serving any catalog model ('gateway', '*') takes any agent's work.
AND ($2='gateway' OR (a.model=$1 AND COALESCE(NULLIF(a.settings->>'provider',''),$2)=$2))
AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs other
 WHERE other.workspace=j.workspace AND other.payload->>'agent'=j.payload->>'agent' AND other.reply<>j.reply
 AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)
-- A request whose audio is still being transcribed waits for it, at most 30
-- seconds after it was asked; then it runs and the agent sees "pending"
-- (schema 95). Waiting here holds no executor slot.
AND NOT EXISTS(SELECT 1 FROM relay_chat_messages asked
 CROSS JOIN LATERAL jsonb_array_elements_text(asked.attachments) AS attached(object)
 JOIN relay_object_transcripts t ON t.workspace=asked.workspace AND t.object=attached.object
 WHERE j.created_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint-30000
 AND asked.workspace=j.workspace AND asked.id=j.request_message AND t.status='pending')
-- It also waits, under the same limit, for files being read that were shared
-- in its conversation in the two minutes before it: its own, or a file sent
-- and then asked about (schema 100). Only pending readings are scanned.
AND NOT EXISTS(SELECT 1 FROM relay_object_contents c
 JOIN relay_objects o ON o.workspace=c.workspace AND o.id=c.object
 JOIN relay_chat_messages asked ON asked.workspace=j.workspace AND asked.id=j.request_message
 WHERE c.status='pending' AND c.workspace=j.workspace AND o.chat=asked.chat
 AND c.created_ms BETWEEN j.created_ms-120000 AND j.created_ms
 AND j.created_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint-30000
 AND EXISTS(SELECT 1 FROM relay_chat_messages shared WHERE shared.workspace=o.workspace
  AND shared.chat=o.chat AND shared.attachments @> jsonb_build_array(o.id)))
AND (SELECT active FROM model_load)<$4
AND COALESCE(w.active,0)<$3
AND (j.work_class='interactive' OR (
 $6<$5-$7 AND (SELECT active FROM model_load)<$4-$7
 AND (SELECT ready FROM ready_capacity)>$7
 AND COALESCE(w.active,0)<$3-$7
))
-- After three interactive offers, give one eligible background request a turn.
-- The background branch above still cannot consume reserved capacity.
ORDER BY CASE WHEN (j.work_class='background') =
 (COALESCE((SELECT interactive_streak FROM relay_pool_turns WHERE provider=$2 AND model=$1),0)>=3) THEN 0 ELSE 1 END,
 COALESCE(w.active,0),
 COALESCE((SELECT max(history.created_ms) FROM relay_worker_assignments history WHERE history.workspace=j.workspace),0),j.sequence
FOR UPDATE OF j SKIP LOCKED LIMIT 1
