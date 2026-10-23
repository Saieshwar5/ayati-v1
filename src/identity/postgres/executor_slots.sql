ALTER TABLE relay_chat_jobs DROP CONSTRAINT relay_chat_jobs_dispatch_check;
ALTER TABLE relay_chat_jobs ADD CONSTRAINT relay_chat_jobs_dispatch_check
 CHECK(dispatch IN ('polling','assigned','pooled'));
ALTER TABLE relay_executors ADD COLUMN pooled BOOLEAN NOT NULL DEFAULT FALSE,
 ADD COLUMN state TEXT NOT NULL DEFAULT 'serving' CHECK(state IN ('serving','draining','lost','stopped')),
 ADD COLUMN heartbeat BIGINT NOT NULL DEFAULT floor(extract(epoch FROM clock_timestamp()))::bigint;
ALTER TABLE relay_assignment_credentials DROP CONSTRAINT relay_assignment_credentials_slot_key;
ALTER TABLE relay_assignment_credentials ADD COLUMN slot_generation BIGINT NOT NULL DEFAULT 0 CHECK(slot_generation>=0);
CREATE UNIQUE INDEX relay_capability_slot_generation ON relay_assignment_credentials(slot,slot_generation);
CREATE TABLE relay_executor_slots (
 executor TEXT NOT NULL,
 incarnation TEXT NOT NULL,
 slot TEXT NOT NULL,
 generation BIGINT NOT NULL CHECK(generation>=1),
 provider TEXT NOT NULL,
 model TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('preparing','ready','offered','running','cleaning','draining','lost')),
 token_hash TEXT NOT NULL CHECK(length(token_hash)=43),
 pid BIGINT,
 assignment TEXT REFERENCES relay_worker_assignments(id),
 ready_ms BIGINT,
 PRIMARY KEY(executor,incarnation,slot),
 FOREIGN KEY(executor,incarnation) REFERENCES relay_executors(id,incarnation)
);
CREATE INDEX relay_slots_ready ON relay_executor_slots(state,provider,model);
CREATE INDEX relay_assignments_workspace_dispatch ON relay_worker_assignments(workspace,created_ms);
