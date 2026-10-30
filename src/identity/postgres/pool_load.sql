-- Shared by the chat and task candidate queries, under the global admission
-- lock. Running chat jobs and running tasks both count toward every limit.
WITH model_load AS (
 SELECT (SELECT count(*) FROM relay_chat_jobs other
  LEFT JOIN relay_assignment_credentials c ON c.assignment=other.claim
  LEFT JOIN relay_executor_slots owned ON owned.executor=c.executor AND owned.incarnation=c.incarnation
   AND owned.slot=c.slot AND owned.generation=c.slot_generation
  LEFT JOIN relay_shared_agents oa ON oa.workspace=other.workspace AND oa.id=other.payload->>'agent'
  WHERE other.state='running' AND other.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
  -- Active work consumes its assigned profile even if the agent is edited later.
  AND COALESCE(owned.model,c.context->>'model',oa.model)=$1
  AND COALESCE(owned.provider,NULLIF(c.context->>'provider',''),NULLIF(oa.settings->>'provider',''),$2)=$2)
 +(SELECT count(*) FROM relay_executor_tasks t
  WHERE t.state='running' AND t.lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
  AND t.model=$1 AND COALESCE(t.provider,$2)=$2) AS active
), ready_capacity AS (
 SELECT count(*) AS ready FROM relay_executor_slots s
 JOIN relay_executors e ON e.id=s.executor AND e.incarnation=s.incarnation
 JOIN jsonb_to_recordset($8::jsonb) AS connected(executor text,incarnation text)
 ON connected.executor=e.id AND connected.incarnation=e.incarnation
 WHERE s.state='ready' AND s.preparation_version=1 AND e.state='serving'
 AND e.expires>floor(extract(epoch FROM clock_timestamp()))::bigint
 AND s.model=$1 AND s.provider=$2
), workspace_load AS (
 SELECT workspace,count(*) AS active FROM (
  SELECT workspace FROM relay_chat_jobs WHERE state='running'
   AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
  UNION ALL
  SELECT workspace FROM relay_executor_tasks WHERE state='running'
   AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint
 ) running GROUP BY workspace
)
