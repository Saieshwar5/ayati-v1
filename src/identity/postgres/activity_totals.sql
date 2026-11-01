WITH v AS (
 SELECT j.reply,j.payload->>'requester' AS requester,j.created_ms AS created_ms,j.state AS job_state,m.state,m.chat,
  c.kind AS conversation_kind,k.kind,ag.id AS agent,ag.name AS agent_name,ag.settings->>'avatar' AS avatar
 /*FILTER*/
 AND $12::bigint>0
), actions AS (
 SELECT x.* FROM relay_control_actions x JOIN v ON v.reply=x.job WHERE x.workspace=$1
), unsure AS (
 SELECT DISTINCT job FROM actions
 WHERE state='uncertain' OR (state='executing' AND updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60)
)
SELECT jsonb_build_object(
 'requests',(SELECT count(*) FROM v),
 'done',(SELECT count(*) FROM v WHERE job_state='finished' AND state='complete'),
 'failed',(SELECT count(*) FROM v WHERE state IN ('failed','interrupted')),
 'review',(SELECT count(*) FROM v WHERE state IN ('failed','interrupted') OR reply IN (SELECT job FROM unsure)),
 'waiting',(SELECT count(*) FROM v WHERE job_state='waiting'),
 'automatic',(SELECT count(*) FROM actions WHERE state='completed' AND NOT requires_approval),
 'approved',(SELECT count(*) FROM actions WHERE state='completed' AND decision='approve'),
 'people',(SELECT count(DISTINCT requester) FROM v),
 'channels',(SELECT count(DISTINCT chat) FROM v WHERE conversation_kind<>'direct'),
 'schedule_runs',(SELECT count(*) FROM v WHERE kind='schedule'),
 'per_day',(SELECT COALESCE(jsonb_agg(jsonb_build_object('day',d,'count',n) ORDER BY d),'[]'::jsonb) FROM (
   SELECT to_char(to_timestamp(created_ms/1000.0+$14*60) AT TIME ZONE 'UTC','YYYY-MM-DD') AS d,count(*) AS n FROM v GROUP BY 1) days),
 'by_agent',(SELECT COALESCE(jsonb_agg(jsonb_build_object('agent',agent,'name',agent_name,'avatar',avatar,'count',n) ORDER BY n DESC,agent_name),'[]'::jsonb) FROM (
   SELECT agent,agent_name,avatar,count(*) AS n FROM v GROUP BY 1,2,3 ORDER BY n DESC,agent_name LIMIT 12) agents)
) AS data
