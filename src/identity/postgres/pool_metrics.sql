-- Waiting work is pooled chat replies plus queued executor tasks.
WITH waiting AS (
 SELECT created_ms FROM relay_chat_jobs WHERE dispatch='pooled' AND state='queued'
 UNION ALL
 SELECT created_ms FROM relay_executor_tasks WHERE state='queued'
), slots AS (
 SELECT s.state FROM relay_executor_slots s
 JOIN relay_executors e ON e.id=s.executor AND e.incarnation=s.incarnation
 WHERE e.state='serving' AND e.expires>floor(extract(epoch FROM clock_timestamp()))::bigint
)
SELECT (SELECT count(*) FROM waiting) AS queued,
 COALESCE((SELECT floor(extract(epoch FROM clock_timestamp())) - min(created_ms)/1000 FROM waiting), 0)::bigint AS oldest_queued_seconds,
 (SELECT count(*) FROM slots WHERE state='ready') AS ready_slots,
 (SELECT count(*) FROM slots WHERE state IN ('offered','running')) AS busy_slots,
 (SELECT count(*) FROM relay_executors WHERE state='serving'
  AND expires>floor(extract(epoch FROM clock_timestamp()))::bigint) AS executors,
 (SELECT count(*) FROM pg_stat_activity WHERE datname=current_database()) AS connections,
 pg_database_size(current_database()) AS database_bytes
