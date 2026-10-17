PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS identity(id INTEGER PRIMARY KEY CHECK(id=1), user_id TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS connections(
 id TEXT PRIMARY KEY, toolkit TEXT NOT NULL, label TEXT NOT NULL,
 account_id TEXT UNIQUE, auth_config_id TEXT NOT NULL,
 status TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS grants(
 agent_id TEXT NOT NULL, connection_id TEXT NOT NULL REFERENCES connections(id),
 tool TEXT NOT NULL, version TEXT NOT NULL, definition TEXT NOT NULL,
 PRIMARY KEY(agent_id,connection_id,tool)
);
CREATE TABLE IF NOT EXISTS executions(
 id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, task_id TEXT NOT NULL,
 connection_id TEXT NOT NULL, tool TEXT NOT NULL, status TEXT NOT NULL,
 log_id TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
