-- Schema 59: living Home panels (people-and-agents-first, Phase 3 HP2).
-- A panel with an update agreement also updates when its agent finishes work that
-- touched one of its source connections, or when an event arrives on one; at most
-- once per min_gap_seconds. Requests merge into one wanted row per panel.
ALTER TABLE relay_home_maintenance
 ADD COLUMN follow_sources BOOLEAN NOT NULL DEFAULT true,
 ADD COLUMN min_gap_seconds BIGINT NOT NULL DEFAULT 300 CHECK(min_gap_seconds BETWEEN 60 AND 86400);
CREATE TABLE relay_home_update_wanted (
 workspace TEXT NOT NULL, panel TEXT NOT NULL,
 reason TEXT NOT NULL CHECK(reason IN ('work','event')),
 cause TEXT NOT NULL CHECK(char_length(cause) BETWEEN 1 AND 200),
 due_ms BIGINT NOT NULL CHECK(due_ms>0),
 detail TEXT NOT NULL DEFAULT '' CHECK(char_length(detail)<=300),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,panel),
 FOREIGN KEY(workspace,panel) REFERENCES relay_home_panels(workspace,id)
);
CREATE INDEX relay_home_update_wanted_due ON relay_home_update_wanted(due_ms);
-- Why an update ran: a person (Update now), its schedule, the agent's work or an event.
ALTER TABLE relay_home_update_runs
 ADD COLUMN reason TEXT CHECK(reason IN ('work','event')),
 ADD COLUMN cause TEXT CHECK(cause IS NULL OR char_length(cause) BETWEEN 1 AND 200);
-- HP3: how the panel's numbers come from its reviewed reads. Kept only when they
-- reproduced an update exactly; then the controller refreshes the numbers itself and
-- the agent is called only when something meaningful changed or a binding fails.
ALTER TABLE relay_home_maintenance
 ADD COLUMN bindings JSONB CHECK(bindings IS NULL OR (jsonb_typeof(bindings)='array' AND jsonb_array_length(bindings) BETWEEN 1 AND 16));
ALTER TABLE relay_home_update_runs
 ADD COLUMN direct BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN fallback TEXT CHECK(fallback IS NULL OR char_length(fallback)<=300);
