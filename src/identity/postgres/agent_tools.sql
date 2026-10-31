-- An agent's Tools tab: every approved, connected connector, whether the agent
-- uses it, and per action the workspace rule, the agent's own rule and the
-- result. Defaults match relay_control_effective_rules: reads Allow, writes Ask.
SELECT jsonb_build_object(
 'id',c.id,'server',c.server,'name',c.definition->>'name','account',c.definition->>'account',
 'revision',c.revision,'on',ac.agent IS NOT NULL,
 'operations',(SELECT COALESCE(jsonb_agg(jsonb_build_object(
   'id',o.op->>'id','title',o.op->>'title','effect',o.op->>'effect',
   'workspace_mode',COALESCE(w.mode,CASE WHEN o.op->>'effect'='read' THEN 'allow' ELSE 'ask' END),
   'needs_other',COALESCE(w.needs_other,false),'agent_mode',r.mode,'revision',COALESCE(r.revision,0),
   'mode',relay_stricter(COALESCE(w.mode,CASE WHEN o.op->>'effect'='read' THEN 'allow' ELSE 'ask' END),COALESCE(r.mode,'allow'))
  ) ORDER BY o.ord),'[]'::jsonb)
  FROM jsonb_array_elements(c.definition->'operations') WITH ORDINALITY AS o(op,ord)
  LEFT JOIN relay_connector_rules w ON w.workspace=c.workspace AND w.connection=c.id AND w.operation=o.op->>'id'
  LEFT JOIN relay_control_rules r ON r.workspace=c.workspace AND r.agent=$2 AND r.connection=c.id AND r.operation=o.op->>'id')
) AS data
FROM relay_control_connections c
JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server
LEFT JOIN relay_agent_connections ac ON ac.workspace=c.workspace AND ac.connection=c.id AND ac.agent=$2
WHERE c.workspace=$1 AND c.enabled AND s.approved
ORDER BY c.server
