-- Provider operations (schema 110): a row per gateway call through a connector
-- provider, the circuit breaker per provider that every controller shares, and
-- the drift check's runs and results. Counts, names and ids only: never
-- arguments, results or tokens.

-- One row per call `control::dispatch_with` handled. `outcome` is NULL while
-- the call is running. `provider_error` marks a provider that failed or did not
-- answer while Cuckoo checked the account and action (before anything was sent).
CREATE TABLE relay_connector_calls (
 id TEXT PRIMARY KEY,
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 action TEXT NOT NULL, agent TEXT NOT NULL, connection TEXT NOT NULL,
 app TEXT NOT NULL,
 provider TEXT NOT NULL CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 operation TEXT NOT NULL,
 started_ms BIGINT NOT NULL,
 latency_ms BIGINT CHECK(latency_ms >= 0),
 outcome TEXT CHECK(outcome IN ('completed','uncertain','refused','preflight_failed')),
 provider_error BOOLEAN NOT NULL DEFAULT false,
 -- Why Cuckoo refused it: its switch, the breaker or the rate limit.
 refused TEXT CHECK(refused IN ('paused','breaker','rate_limit')),
 usd_estimate DOUBLE PRECISION NOT NULL DEFAULT 0 CHECK(usd_estimate >= 0),
 CHECK((outcome = 'refused') = (refused IS NOT NULL))
);
-- The rate limit counts a workspace's recent calls to one app.
CREATE INDEX relay_connector_calls_rate ON relay_connector_calls(workspace,app,started_ms);
-- The Apps page shows when each account was last used, and by which agent.
CREATE INDEX relay_connector_calls_used ON relay_connector_calls(workspace,connection,started_ms DESC)
 WHERE outcome IN ('completed','uncertain');
-- Cuckoo Control's health: calls, errors and cost per provider this month.
CREATE INDEX relay_connector_calls_provider ON relay_connector_calls(provider,started_ms);

-- The circuit breaker per provider. Closed while open_until_ms is 0; open while
-- it is in the future; once it has passed, one trial call (`probe`, leased
-- until probe_until_ms) may go, and its result closes or reopens the breaker.
CREATE TABLE relay_connector_breakers (
 provider TEXT PRIMARY KEY CHECK(provider ~ '^[a-z][a-z0-9_]{0,31}$'),
 failures INTEGER NOT NULL DEFAULT 0 CHECK(failures >= 0),
 window_ms BIGINT NOT NULL DEFAULT 0,
 open_until_ms BIGINT NOT NULL DEFAULT 0,
 probe TEXT, probe_until_ms BIGINT NOT NULL DEFAULT 0,
 opened_ms BIGINT, changed_ms BIGINT NOT NULL
);

-- Drift check runs: nightly on one controller (leased), or `ayati admin
-- connectors verify`. A run whose lease passed without finishing was abandoned.
CREATE TABLE relay_connector_drift_runs (
 id TEXT PRIMARY KEY,
 via TEXT NOT NULL CHECK(via IN ('nightly','cli')),
 started_ms BIGINT NOT NULL, lease_until_ms BIGINT NOT NULL, finished_ms BIGINT,
 accounts INTEGER NOT NULL DEFAULT 0, skipped INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX relay_connector_drift_runs_started ON relay_connector_drift_runs(started_ms DESC);
-- One result per account and reviewed action. `fingerprint` is the account's
-- when it was checked, so reconnecting (a new review) retires old results.
CREATE TABLE relay_connector_drift (
 run TEXT NOT NULL REFERENCES relay_connector_drift_runs(id) ON DELETE CASCADE,
 workspace TEXT NOT NULL, connection TEXT NOT NULL, fingerprint TEXT NOT NULL,
 provider TEXT NOT NULL, operation TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('ok','missing','changed','new_version','failed')),
 fields JSONB NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(fields) = 'array'),
 version TEXT,
 PRIMARY KEY(run,workspace,connection,operation),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id)
);
CREATE INDEX relay_connector_drift_connection ON relay_connector_drift(workspace,connection);
