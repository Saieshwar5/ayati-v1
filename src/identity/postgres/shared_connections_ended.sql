-- Schema 133: every app connection belongs to an agent.
--
-- Schema 132 let an agent own its connections and left the older ones, which
-- belonged to the workspace and were shared by its agents, working. Those end
-- here: they are switched off as a new revision, and every agent's switch and
-- rule for them is removed, so work waiting on them is cancelled and nothing
-- more is sent through them. An agent's owner connects the app again on the
-- agent's Connections tab. A connected system (an MCP server added on one
-- agent's page) is already tied to its agent and is untouched. The accounts
-- stay at their provider until someone removes them there.
DELETE FROM relay_agent_connections ac USING relay_control_connections c
 WHERE c.workspace=ac.workspace AND c.id=ac.connection AND c.agent IS NULL AND c.provider<>'system';
DELETE FROM relay_control_rules r USING relay_control_connections c
 WHERE c.workspace=r.workspace AND c.id=r.connection AND c.agent IS NULL AND c.provider<>'system';
DELETE FROM relay_connector_rules r USING relay_control_connections c
 WHERE c.workspace=r.workspace AND c.id=r.connection AND c.agent IS NULL AND c.provider<>'system';
UPDATE relay_control_connections SET enabled=false,revision=revision+1
 WHERE agent IS NULL AND provider<>'system' AND enabled;
