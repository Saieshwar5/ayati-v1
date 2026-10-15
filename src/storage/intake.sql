CREATE TABLE dispatch_control (
    id INTEGER PRIMARY KEY CHECK(id=1),
    paused INTEGER NOT NULL DEFAULT 0 CHECK(paused IN (0,1))
);
INSERT INTO dispatch_control(id) VALUES(1);
CREATE TABLE event_receipts (
    source TEXT NOT NULL,
    event_id TEXT NOT NULL,
    task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id),
    PRIMARY KEY(source,event_id)
);
CREATE TABLE schedules (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    run_at_ms INTEGER NOT NULL,
    interval_seconds INTEGER,
    next_run_ms INTEGER,
    state TEXT NOT NULL CHECK(state IN ('active','paused','completed')),
    last_task_id TEXT REFERENCES tasks(id)
);
CREATE TABLE schedule_runs (
    schedule_id TEXT NOT NULL REFERENCES schedules(id),
    due_at_ms INTEGER NOT NULL,
    task_id TEXT NOT NULL UNIQUE REFERENCES tasks(id),
    PRIMARY KEY(schedule_id,due_at_ms)
);
CREATE INDEX due_schedules ON schedules(next_run_ms) WHERE state='active';
PRAGMA user_version = 4;
