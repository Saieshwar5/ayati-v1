PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;
CREATE TABLE IF NOT EXISTS identity(id INTEGER PRIMARY KEY CHECK(id=1), user_id TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS agent_authority(agent_id TEXT PRIMARY KEY,revision INTEGER NOT NULL);
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
CREATE TABLE IF NOT EXISTS consumer_tools(connection_id TEXT NOT NULL REFERENCES connections(id),tool TEXT NOT NULL,version TEXT NOT NULL,PRIMARY KEY(connection_id,tool));
CREATE TABLE IF NOT EXISTS executions(
 id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, task_id TEXT NOT NULL,
 connection_id TEXT NOT NULL, tool TEXT NOT NULL, status TEXT NOT NULL,
 log_id TEXT, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE IF NOT EXISTS triggers(
 id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, connection_id TEXT NOT NULL REFERENCES connections(id),
 name TEXT NOT NULL, slug TEXT NOT NULL, provider_id TEXT NOT NULL,
 instructions TEXT NOT NULL, filter TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('active','paused','removed')), revision INTEGER NOT NULL DEFAULT 1,
 UNIQUE(agent_id,connection_id,provider_id)
);
CREATE TABLE IF NOT EXISTS webhook_receipts(id TEXT PRIMARY KEY, body TEXT NOT NULL, received_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
CREATE TABLE IF NOT EXISTS trigger_deliveries(
 id TEXT PRIMARY KEY, trigger_id TEXT NOT NULL REFERENCES triggers(id),
 event_id TEXT NOT NULL REFERENCES webhook_receipts(id), content TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending', task_id TEXT, rule_revision INTEGER NOT NULL DEFAULT 1,
 UNIQUE(trigger_id,event_id)
);
CREATE INDEX IF NOT EXISTS pending_trigger_deliveries ON trigger_deliveries(state,trigger_id);
CREATE INDEX IF NOT EXISTS agent_triggers ON triggers(agent_id,state);
CREATE INDEX IF NOT EXISTS event_trigger_routes ON triggers(provider_id,slug,connection_id);
CREATE TABLE IF NOT EXISTS panels(
 id TEXT PRIMARY KEY, agent_id TEXT NOT NULL, revision INTEGER NOT NULL,
 active_revision INTEGER, state TEXT NOT NULL DEFAULT 'saved', updates_enabled INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS panel_versions(
 panel_id TEXT NOT NULL REFERENCES panels(id), revision INTEGER NOT NULL,
 definition TEXT NOT NULL, sources TEXT NOT NULL, task_id TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 PRIMARY KEY(panel_id,revision)
);
CREATE TABLE IF NOT EXISTS panel_schedules(panel_id TEXT NOT NULL REFERENCES panels(id),agent_id TEXT NOT NULL,schedule_id TEXT NOT NULL,PRIMARY KEY(panel_id,schedule_id),UNIQUE(agent_id,schedule_id));
CREATE TABLE IF NOT EXISTS apps(id TEXT PRIMARY KEY,agent_id TEXT NOT NULL,revision INTEGER NOT NULL,active_revision INTEGER,state TEXT NOT NULL DEFAULT 'saved');
CREATE TABLE IF NOT EXISTS app_versions(app_id TEXT NOT NULL REFERENCES apps(id),revision INTEGER NOT NULL,definition TEXT NOT NULL,task_id TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,PRIMARY KEY(app_id,revision));
CREATE TABLE IF NOT EXISTS app_sessions(id TEXT PRIMARY KEY,app_id TEXT NOT NULL REFERENCES apps(id),revision INTEGER NOT NULL,preview INTEGER NOT NULL,expires_ms INTEGER NOT NULL,document_id TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS app_schemas(app_id TEXT PRIMARY KEY REFERENCES apps(id),collections TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS app_records(app_id TEXT NOT NULL REFERENCES apps(id),dataset TEXT NOT NULL,collection TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,data TEXT NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(app_id,dataset,collection,id));
CREATE TABLE IF NOT EXISTS app_record_receipts(app_id TEXT NOT NULL,dataset TEXT NOT NULL,request_id TEXT NOT NULL,command TEXT NOT NULL,result TEXT NOT NULL,PRIMARY KEY(app_id,dataset,request_id));
CREATE TABLE IF NOT EXISTS app_record_history(app_id TEXT NOT NULL,dataset TEXT NOT NULL,collection TEXT NOT NULL,id TEXT NOT NULL,revision INTEGER NOT NULL,data TEXT NOT NULL,deleted INTEGER NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,PRIMARY KEY(app_id,dataset,collection,id,revision));
CREATE TABLE IF NOT EXISTS app_grants(app_id TEXT NOT NULL REFERENCES apps(id),agent_id TEXT NOT NULL,collection TEXT NOT NULL,can_write INTEGER NOT NULL,PRIMARY KEY(app_id,agent_id,collection));
CREATE TABLE IF NOT EXISTS app_observations(id TEXT PRIMARY KEY,agent_id TEXT NOT NULL,task_id TEXT NOT NULL,app_id TEXT NOT NULL,collection TEXT NOT NULL,created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
