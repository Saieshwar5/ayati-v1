-- Called under the global admission lock. Class is immutable controller intake
-- metadata. Connection membership is a bounded live hint, never authority.
WITH model_load AS (
 SELECT count(*) AS active FROM relay_chat_jobs other
 LEFT JOIN relay_assignment_credentials c ON c.assignment=other.claim
 LEFT JOIN relay_executor_slots owned ON owned.executor=c.executor AND owned.incarnation=c.incarnation
  AND owned.slot=c.slot AND owned.generation=c.slot_generation
 LEFT JOIN relay_shared_agents oa ON oa.workspace=other.workspace AND oa.id=other.payload->>'agent'
 WHERE other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
 -- Active work consumes its assigned profile even if the agent is edited later.
 AND COALESCE(owned.model,c.context->>'model',oa.model)=$1
 AND COALESCE(owned.provider,NULLIF(c.context->>'provider',''),NULLIF(oa.settings->>'provider',''),$2)=$2
), ready_capacity AS (
 SELECT count(*) AS ready FROM relay_executor_slots s
 JOIN relay_executors e ON e.id=s.executor AND e.incarnation=s.incarnation
 JOIN jsonb_to_recordset($8::jsonb) AS connected(executor text,incarnation text)
 ON connected.executor=e.id AND connected.incarnation=e.incarnation
 WHERE s.state='ready' AND s.preparation_version=1 AND e.state='serving'
 AND e.expires>floor(extract(epoch FROM clock_timestamp()))::bigint
 AND s.model=$1 AND s.provider=$2
)
SELECT j.reply,j.workspace,j.attempts,j.work_class FROM relay_chat_jobs j
JOIN relay_shared_agents a ON a.workspace=j.workspace AND a.id=j.payload->>'agent'
WHERE j.dispatch='pooled' AND j.attempts<3
AND (j.state='queued' OR (j.state='running' AND j.lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint))
AND a.model=$1 AND COALESCE(NULLIF(a.settings->>'provider',''),$2)=$2
AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs other
 WHERE other.workspace=j.workspace AND other.payload->>'agent'=j.payload->>'agent' AND other.reply<>j.reply
 AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)
AND (SELECT active FROM model_load)<$4
AND (SELECT count(*) FROM relay_chat_jobs other WHERE other.workspace=j.workspace
 AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)<$3
AND (j.work_class='interactive' OR (
 $6<$5-$7 AND (SELECT active FROM model_load)<$4-$7
 AND (SELECT ready FROM ready_capacity)>$7
 AND (SELECT count(*) FROM relay_chat_jobs other WHERE other.workspace=j.workspace
  AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint)<$3-$7
))
-- After three interactive offers, give one eligible background request a turn.
-- The background branch above still cannot consume reserved capacity.
ORDER BY CASE WHEN (j.work_class='background') =
 (COALESCE((SELECT interactive_streak FROM relay_pool_turns WHERE provider=$2 AND model=$1),0)>=3) THEN 0 ELSE 1 END,
 (SELECT count(*) FROM relay_chat_jobs other WHERE other.workspace=j.workspace
  AND other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint),
 COALESCE((SELECT max(history.created_ms) FROM relay_worker_assignments history WHERE history.workspace=j.workspace),0),j.sequence
FOR UPDATE OF j SKIP LOCKED LIMIT 1
