-- Appended to pool_load.sql. Class is immutable controller intake metadata.
-- Connection membership is a bounded live hint, never authority.
SELECT j.reply,j.workspace,j.attempts,j.generation,j.work_class FROM relay_chat_jobs j
JOIN relay_shared_agents a ON a.workspace=j.workspace AND a.id=j.payload->>'agent'
LEFT JOIN workspace_load w ON w.workspace=j.workspace
WHERE j.dispatch='pooled' AND j.attempts<3
AND (j.state='queued' OR (j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint))
AND a.model=$1 AND COALESCE(NULLIF(a.settings->>'provider',''),$2)=$2
AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs other
 WHERE other.workspace=j.workspace AND other.payload->>'agent'=j.payload->>'agent' AND other.reply<>j.reply
 AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)
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
