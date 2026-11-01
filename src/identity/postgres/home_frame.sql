-- The panel frame (people-and-agents-first, Phase 1e): each published update may
-- carry the agent's short "what changed" note, shown on the panel. Optional, so
-- older snapshots and updates without a meaningful change have none.
ALTER TABLE relay_home_snapshots ADD COLUMN what_changed TEXT
 CHECK(what_changed IS NULL OR char_length(what_changed) BETWEEN 1 AND 280);
