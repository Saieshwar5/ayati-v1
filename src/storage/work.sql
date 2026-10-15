CREATE TABLE input_receipts (
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    request_id TEXT NOT NULL,
    task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id),
    accepted_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    PRIMARY KEY (conversation_id, request_id)
);
CREATE TABLE run_claims (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    token TEXT NOT NULL UNIQUE,
    owner_id TEXT NOT NULL,
    started_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE work_transitions (
    id INTEGER PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    from_status TEXT,
    to_status TEXT NOT NULL,
    detail TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TRIGGER record_work_transition AFTER UPDATE OF status ON tasks
WHEN OLD.status != NEW.status BEGIN
    INSERT INTO work_transitions(task_id,from_status,to_status,detail)
    VALUES(NEW.id,OLD.status,NEW.status,NEW.detail);
END;
ALTER TABLE permissions ADD COLUMN run_token TEXT;
ALTER TABLE tool_attempts ADD COLUMN run_token TEXT;
ALTER TABLE tool_attempts ADD COLUMN permission_id TEXT REFERENCES permissions(id);
CREATE UNIQUE INDEX runtime_tool_identity ON tool_attempts(task_id,call_id)
    WHERE run_token IS NOT NULL;
-- Old queued records were accepted by a non-durable dispatcher. Do not replay them.
UPDATE tasks SET status='interrupted',detail='Legacy queued work needs deliberate follow-up.'
    WHERE status='queued';
PRAGMA user_version = 3;
