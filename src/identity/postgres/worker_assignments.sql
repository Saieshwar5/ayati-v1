ALTER TABLE relay_chat_jobs
 ADD COLUMN dispatch TEXT NOT NULL DEFAULT 'polling' CHECK(dispatch IN ('polling','assigned')),
 ADD COLUMN run_seconds INTEGER NOT NULL DEFAULT 180 CHECK(run_seconds BETWEEN 1 AND 86400),
 ADD CONSTRAINT relay_jobs_workspace_reply UNIQUE(workspace,reply);
CREATE INDEX relay_jobs_dispatch_ready ON relay_chat_jobs(dispatch,state,lease_until,sequence);
CREATE INDEX relay_jobs_agent_running ON relay_chat_jobs(workspace,(payload->>'agent'),lease_until)
 WHERE state='running';

-- This is a launch/attempt ledger, not another source of agent identity or authority.
-- The assignment ID is an idempotent local launch identity, never a bearer credential.
CREATE TABLE relay_worker_assignments (
 id TEXT PRIMARY KEY,
 workspace TEXT NOT NULL,
 reply TEXT NOT NULL,
 generation INTEGER NOT NULL CHECK(generation BETWEEN 1 AND 3),
 provider TEXT NOT NULL CHECK(provider='local'),
 resource_id TEXT,
 state TEXT NOT NULL CHECK(state IN ('launching','running','finished','lost')),
 outcome TEXT CHECK(outcome IN ('complete','failed','cancelled','interrupted','lost','launch_failed')),
 lease_seconds INTEGER NOT NULL CHECK(lease_seconds BETWEEN 6 AND 300),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 started_ms BIGINT,
 heartbeat_ms BIGINT,
 finished_ms BIGINT,
 UNIQUE(reply,generation),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE INDEX relay_assignments_active ON relay_worker_assignments(state,created_ms);
