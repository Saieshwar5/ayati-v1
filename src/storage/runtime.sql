CREATE TABLE IF NOT EXISTS businesses (id TEXT PRIMARY KEY, name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    business_id TEXT NOT NULL REFERENCES businesses(id),
    name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id)
);
INSERT OR IGNORE INTO businesses VALUES ('local-business','Local test business');
INSERT OR IGNORE INTO agents VALUES ('local-agent','local-business','Ayati');
INSERT OR IGNORE INTO conversations VALUES ('local-conversation','local-agent');
CREATE TABLE IF NOT EXISTS run_scopes (
    task_id TEXT PRIMARY KEY REFERENCES tasks(id),
    conversation_id TEXT NOT NULL REFERENCES conversations(id)
);
CREATE TABLE IF NOT EXISTS runtime_sessions (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id),
    session_id TEXT NOT NULL,
    runtime_version TEXT NOT NULL,
    settings TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS permissions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id),
    title TEXT NOT NULL,
    action TEXT NOT NULL,
    decision TEXT NOT NULL DEFAULT 'pending'
        CHECK(decision IN ('pending','allowed','denied','cancelled'))
);
PRAGMA user_version = 2;
