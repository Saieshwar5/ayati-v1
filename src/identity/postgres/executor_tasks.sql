-- Dispatch and lease ledger for one-off, tool-free executor work that is not a
-- chat reply. Each purpose keeps its own request, context and result (samples in
-- relay_agent_rehearsals, setup suggestions in relay_control_drafts). Only executors run tasks, and a task never
-- automatically reruns inference.
CREATE TABLE relay_executor_tasks (
 id TEXT PRIMARY KEY,
 workspace TEXT NOT NULL,
 purpose TEXT NOT NULL CHECK(purpose IN ('sample','setup_suggestion')),
 agent TEXT NOT NULL, actor TEXT NOT NULL REFERENCES relay_users(id),
 model TEXT NOT NULL, provider TEXT,
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','finished')),
 claim TEXT, lease_until BIGINT NOT NULL DEFAULT 0,
 sequence BIGINT GENERATED ALWAYS AS IDENTITY,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 finished_ms BIGINT,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 CHECK((state='running')=(claim IS NOT NULL)),
 CHECK((state='finished')=(finished_ms IS NOT NULL))
);
CREATE INDEX relay_executor_tasks_ready ON relay_executor_tasks(model,sequence) WHERE state='queued';
CREATE INDEX relay_executor_tasks_running ON relay_executor_tasks(workspace,lease_until)
 WHERE state='running';

-- Setup suggestions keep their tool-free context with the draft while pending.
ALTER TABLE relay_control_drafts ADD COLUMN suggestion_context JSONB
 CHECK(octet_length(suggestion_context::text)<=262144);

-- An assignment produces exactly one chat reply or one task.
ALTER TABLE relay_worker_assignments
 ALTER COLUMN reply DROP NOT NULL,
 ADD COLUMN task TEXT REFERENCES relay_executor_tasks(id),
 ADD CONSTRAINT relay_assignment_one_work CHECK((reply IS NULL)<>(task IS NULL));
CREATE UNIQUE INDEX relay_assignments_task ON relay_worker_assignments(task) WHERE task IS NOT NULL;
