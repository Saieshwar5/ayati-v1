-- The Apps page (apps-page plan, AP1): every approved, connected service with what
-- agents may do with each action, which agents use it and how many company rules
-- cover it. $2 is true for admins, who also see the account name.
SELECT jsonb_build_object(
 'id',c.id,'server',c.server,'name',c.definition->>'name','kind','service','provider',c.provider,
 'account',CASE WHEN $2 THEN c.definition->>'account' END,
 'status',CASE WHEN EXISTS(SELECT 1 FROM relay_connector_reconnects n WHERE n.workspace=c.workspace AND n.connection=c.id) THEN 'reconnect' ELSE 'connected' END,
 'actions',(SELECT COALESCE(jsonb_agg(jsonb_build_object('id',o.op->>'id','title',o.op->>'title',
   'meaning',relay_action_meaning(c.workspace,c.id,o.op),'mode',COALESCE(w.mode,relay_default_mode(c.workspace,c.id,o.op))) ORDER BY o.ord),'[]'::jsonb)
  FROM jsonb_array_elements(c.definition->'operations') WITH ORDINALITY AS o(op,ord)
  LEFT JOIN relay_connector_rules w ON w.workspace=c.workspace AND w.connection=c.id AND w.operation=o.op->>'id'),
 'agents',(SELECT COALESCE(jsonb_agg(jsonb_build_object('id',a.id,'name',a.name,'avatar',a.settings->>'avatar') ORDER BY a.name),'[]'::jsonb)
  FROM relay_agent_connections ac JOIN relay_shared_agents a ON a.workspace=ac.workspace AND a.id=ac.agent WHERE ac.workspace=c.workspace AND ac.connection=c.id),
 'rules',(SELECT count(*) FROM relay_guard_rules g WHERE g.workspace=c.workspace AND g.enabled AND g.reviewed
  AND ((g.operation IS NOT NULL AND (g.connection IS NULL OR g.connection=c.id) AND EXISTS(SELECT 1 FROM jsonb_array_elements(c.definition->'operations') op WHERE op->>'id'=g.operation))
   OR (g.meaning IS NOT NULL AND EXISTS(SELECT 1 FROM jsonb_array_elements(c.definition->'operations') op WHERE relay_action_meaning(c.workspace,c.id,op)=g.meaning)))),
 -- The latest call that reached the app (schema 110), in milliseconds.
 'last_used',(SELECT jsonb_build_object('at',k.started_ms,'agent_name',a.name) FROM relay_connector_calls k
  JOIN relay_shared_agents a ON a.workspace=k.workspace AND a.id=k.agent
  WHERE k.workspace=c.workspace AND k.connection=c.id AND k.outcome IN ('completed','uncertain') ORDER BY k.started_ms DESC LIMIT 1)
) AS data
FROM relay_control_connections c
JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server AND s.approved
WHERE c.workspace=$1 AND c.enabled
ORDER BY c.definition->>'name', c.id
