-- Pause all agents: an admin's emergency switch. While it is set no agent work
-- starts anywhere in the workspace; work that was running or waiting was stopped
-- when it was set. Each agent's own pause is separate, so resuming leaves agents
-- that were paused on their own still paused.
ALTER TABLE relay_workspaces
 ADD COLUMN agents_paused_ms BIGINT,
 ADD COLUMN agents_paused_by TEXT,
 ADD COLUMN agents_paused_note TEXT CHECK(length(agents_paused_note) <= 300);
