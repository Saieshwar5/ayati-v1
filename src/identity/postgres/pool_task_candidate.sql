-- Appended to pool_load.sql. Tasks are background work: they follow the same
-- rules as background chat and never use capacity reserved for interactive chat.
-- A task whose sample was stopped or invalidated, or whose setup draft no longer
-- waits for suggestions, is never offered.
SELECT t.id,t.workspace FROM relay_executor_tasks t
LEFT JOIN workspace_load w ON w.workspace=t.workspace
WHERE t.state='queued' AND t.model=$1 AND COALESCE(t.provider,$2)=$2
AND (t.purpose<>'sample' OR EXISTS(SELECT 1 FROM relay_agent_rehearsals r
 WHERE r.workspace=t.workspace AND r.id=t.id AND r.state='queued'))
AND (t.purpose<>'setup_suggestion' OR EXISTS(SELECT 1 FROM relay_control_drafts d
 WHERE d.workspace=t.workspace AND d.id=t.id AND d.suggestion_context IS NOT NULL))
AND $6<$5-$7 AND (SELECT active FROM model_load)<$4-$7
AND (SELECT ready FROM ready_capacity)>$7
AND COALESCE(w.active,0)<$3-$7
ORDER BY t.sequence
FOR UPDATE OF t SKIP LOCKED LIMIT 1
