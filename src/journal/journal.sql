-- Private per-run journal. Each Goose run gets a fresh file that is deleted
-- afterwards, so there is one current schema and no upgrade path.
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    status TEXT NOT NULL,
    detail TEXT NOT NULL DEFAULT '',
    context TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE messages (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE conversations (id TEXT PRIMARY KEY);
INSERT INTO conversations VALUES ('local-conversation');
CREATE TABLE run_scopes (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    conversation_id TEXT NOT NULL REFERENCES conversations(id)
);
CREATE TABLE runtime_sessions (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id),
    session_id TEXT NOT NULL,
    runtime_version TEXT NOT NULL,
    settings TEXT NOT NULL
);
CREATE TABLE permissions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    title TEXT NOT NULL,
    action TEXT NOT NULL,
    decision TEXT NOT NULL DEFAULT 'pending'
        CHECK(decision IN ('pending','allowed','denied','cancelled')),
    run_token TEXT
);
CREATE TABLE permission_uses (permission_id TEXT PRIMARY KEY REFERENCES permissions(id));
CREATE TABLE tool_attempts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    call_id TEXT NOT NULL,
    name TEXT NOT NULL,
    arguments TEXT NOT NULL,
    status TEXT NOT NULL,
    result TEXT,
    run_token TEXT,
    permission_id TEXT REFERENCES permissions(id)
);
CREATE UNIQUE INDEX runtime_tool_identity ON tool_attempts(task_id,call_id)
    WHERE run_token IS NOT NULL;
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
PRAGMA user_version = 1;
