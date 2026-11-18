-- Schema 132: an app connection can belong to one agent.
--
-- Until now a service account (Gmail through Composio or Pipedream) belonged
-- to the workspace: an admin connected it once and agents were given a switch
-- on it. A connection with `agent` set belongs to that agent: its owners
-- connect it and set its rules, and no other agent can use it. Several agents
-- can each hold their own account of the same app, so the one-per-app rule
-- becomes one per app and owner. A connection without an agent is the
-- workspace's, as before (operators' own MCP servers, older accounts).
ALTER TABLE relay_control_connections ADD COLUMN agent TEXT;
ALTER TABLE relay_control_connections ADD CONSTRAINT relay_control_connections_agent
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id);
ALTER TABLE relay_control_connections DROP CONSTRAINT relay_control_connections_workspace_server_key;
CREATE UNIQUE INDEX relay_control_connections_owner ON relay_control_connections(workspace,server,COALESCE(agent,''));
-- A sign-in in progress and a one-step connect remember whose connection it is.
ALTER TABLE relay_connector_sign_ins ADD COLUMN agent TEXT;
ALTER TABLE relay_connector_connects ADD COLUMN agent TEXT;
-- Accounts connected before this stay the workspace's until they are connected
-- again for one agent: moving them here would fence the work using them.
