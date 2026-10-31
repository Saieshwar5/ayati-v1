-- Settings → Connections: each connected connector's actions with the workspace
-- rule (or the safe default), and which agents use the connector.
SELECT jsonb_build_object(
 'connection',c.id,
 'agents',(SELECT COALESCE(jsonb_agg(a.name ORDER BY a.name),'[]'::jsonb) FROM relay_agent_connections ac
  JOIN relay_shared_agents a ON a.workspace=ac.workspace AND a.id=ac.agent WHERE ac.workspace=c.workspace AND ac.connection=c.id),
 'operations',(SELECT COALESCE(jsonb_agg(jsonb_build_object(
   'id',o.op->>'id','title',o.op->>'title','effect',o.op->>'effect',
   'mode',COALESCE(w.mode,CASE WHEN o.op->>'effect'='read' THEN 'allow' ELSE 'ask' END),
   'needs_other',COALESCE(w.needs_other,false),'revision',COALESCE(w.revision,0),'default',w.mode IS NULL
  ) ORDER BY o.ord),'[]'::jsonb)
  FROM jsonb_array_elements(c.definition->'operations') WITH ORDINALITY AS o(op,ord)
  LEFT JOIN relay_connector_rules w ON w.workspace=c.workspace AND w.connection=c.id AND w.operation=o.op->>'id')
) AS data
FROM relay_control_connections c WHERE c.workspace=$1 AND c.enabled ORDER BY c.server
