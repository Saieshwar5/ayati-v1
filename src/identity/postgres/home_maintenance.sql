CREATE TABLE relay_home_maintenance (
 workspace TEXT NOT NULL, panel TEXT NOT NULL, revision BIGINT NOT NULL CHECK(revision>0),
 panel_revision BIGINT NOT NULL, enabled BOOLEAN NOT NULL,
 instructions TEXT NOT NULL CHECK(octet_length(instructions) BETWEEN 1 AND 2000),
 interval_seconds BIGINT CHECK(interval_seconds BETWEEN 60 AND 86400),
 stale_seconds BIGINT NOT NULL CHECK(stale_seconds BETWEEN 60 AND 604800),
 scopes JSONB NOT NULL, schedule TEXT,
 generation BIGINT NOT NULL DEFAULT 0 CHECK(generation>=0),
 PRIMARY KEY(workspace,panel),
 FOREIGN KEY(workspace,panel,panel_revision) REFERENCES relay_home_panel_versions(workspace,panel,revision),
 FOREIGN KEY(workspace,schedule) REFERENCES relay_agent_schedules(workspace,id),
 CHECK(interval_seconds IS NULL OR interval_seconds<=stale_seconds)
);
CREATE TABLE relay_home_update_runs (
 workspace TEXT NOT NULL, panel TEXT NOT NULL, reply TEXT NOT NULL,
 policy_revision BIGINT NOT NULL, panel_revision BIGINT NOT NULL,
 expected_generation BIGINT NOT NULL, published_generation BIGINT, request_hash TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,reply),
 FOREIGN KEY(workspace,panel) REFERENCES relay_home_panels(workspace,id),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE INDEX relay_home_update_panel ON relay_home_update_runs(workspace,panel,created_ms DESC);
CREATE TABLE relay_home_schedule_bindings (
 workspace TEXT NOT NULL, schedule TEXT NOT NULL, panel TEXT NOT NULL,
 PRIMARY KEY(workspace,schedule),
 FOREIGN KEY(workspace,schedule) REFERENCES relay_agent_schedules(workspace,id),
 FOREIGN KEY(workspace,panel) REFERENCES relay_home_panels(workspace,id)
);
CREATE TABLE relay_home_snapshots (
 workspace TEXT NOT NULL, panel TEXT NOT NULL, generation BIGINT NOT NULL,
 panel_revision BIGINT NOT NULL, definition JSONB NOT NULL, provenance JSONB NOT NULL,
 task TEXT NOT NULL, checked_ms BIGINT NOT NULL,
 saved_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,panel,generation),
 FOREIGN KEY(workspace,task) REFERENCES relay_home_update_runs(workspace,reply),
 FOREIGN KEY(workspace,panel,panel_revision) REFERENCES relay_home_panel_versions(workspace,panel,revision)
);
CREATE TRIGGER relay_home_snapshot_immutable BEFORE UPDATE ON relay_home_snapshots
 FOR EACH ROW EXECUTE FUNCTION relay_home_version_immutable();
-- Routine data/health changes use Home's existing five-second saved-read poll.
-- Access changes keep the original panel_access invalidation. Reusing that
-- security signal for every snapshot would clear unrelated drafts/readers.
