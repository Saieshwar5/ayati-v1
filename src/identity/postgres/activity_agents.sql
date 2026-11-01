-- Activity → Now: one card per agent the person can see, from the requests they
-- can see ($14 is the start of today in ms). Nothing here is counted from work
-- the person could not open.
WITH v AS (
 SELECT ag.id AS agent,j.reply,j.state AS job_state,m.state,j.created_ms AS created_ms,j.sequence,
  COALESCE(NULLIF(m.progress->>'text',''),(SELECT left(s.content,140) FROM (/*SOURCE*/) s)) AS doing
 /*FILTER*/
 AND $12::bigint>0
 -- Only unfinished work and today's requests matter here.
 AND (j.state<>'finished' OR j.created_ms>=$14)
)
SELECT jsonb_build_object('id',a.id,'name',a.name,'role',a.role,'avatar',a.settings->>'avatar',
 'paused',COALESCE((a.settings->>'paused')::boolean,false),
 'paused_note',(SELECT l.summary FROM relay_audit l WHERE l.workspace=a.workspace AND l.target=a.id
   AND l.action='agent_profile_saved' AND l.summary LIKE '% paused %' ORDER BY l.id DESC LIMIT 1),
 'running',(SELECT count(*) FROM v WHERE v.agent=a.id AND v.job_state IN ('queued','running')),
 'waiting',(SELECT count(*) FROM v WHERE v.agent=a.id AND v.job_state='waiting'),
 'today',(SELECT count(*) FROM v WHERE v.agent=a.id AND v.created_ms>=$14),
 'failed_today',(SELECT count(*) FROM v WHERE v.agent=a.id AND v.created_ms>=$14 AND v.state IN ('failed','interrupted')),
 'doing',(SELECT v.doing FROM v WHERE v.agent=a.id AND v.job_state<>'finished' ORDER BY v.sequence DESC LIMIT 1),
 'next_run',(SELECT min(s.next_ms) FROM relay_agent_schedules s WHERE s.workspace=a.workspace AND s.agent=a.id AND s.state='enabled'),
 -- The latest of today's requests that could not complete, for "How to fix it".
 'last_failed',(SELECT v.reply FROM v WHERE v.agent=a.id AND v.state IN ('failed','interrupted') ORDER BY v.sequence DESC LIMIT 1),
 -- Owners, managers and admins may pause or resume it from the card.
 'can_pause',($3 OR a.owner_id=$2 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2))
) AS data
FROM relay_shared_agents a
WHERE a.workspace=$1 AND a.revision>0
 AND ($3 OR a.owner_id=$2 OR relay_agent_can_talk(a.workspace,a.id,$2)
  OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2))
 -- My agents ($13='mine'): only the agents the person manages.
 AND ($13::text IS DISTINCT FROM 'mine'
  OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2))
ORDER BY a.name,a.id LIMIT 100
