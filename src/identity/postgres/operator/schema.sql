-- Schema 91: Cuckoo Control, the operator console (runtime/cuckoo-control.md).
-- Models, routes, prices, charges, costs and budgets come from schemas 87-90
-- (runtime/model-catalog.md); this adds what operators change by hand.

-- Every change an operator makes, from the web console or `ayati admin`.
-- Append-only: the runtime role may not update or delete rows.
CREATE TABLE relay_operator_log (
 id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor TEXT NOT NULL REFERENCES relay_users(id),
 via TEXT NOT NULL CHECK(via IN ('web','cli')),
 area TEXT NOT NULL CHECK(area IN ('workspaces','providers','settings','codes')),
 action TEXT NOT NULL CHECK(char_length(action) BETWEEN 1 AND 64),
 target TEXT NOT NULL CHECK(char_length(target) BETWEEN 1 AND 200),
 summary TEXT NOT NULL CHECK(char_length(summary) BETWEEN 1 AND 1000),
 reason TEXT NOT NULL CHECK(char_length(reason) BETWEEN 1 AND 300),
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint
);

-- API keys kept in the console, sealed with the deployment's
-- AYATI_SECRETS_KEY_FILE (AES-256-GCM); only the last four characters stay
-- readable. With no base_url the row is the key of a provider in the
-- deployment's providers file (whose address and the catalog stay reviewed
-- files); with one it is a provider added in the console, for agents that name
-- a provider themselves.
CREATE TABLE relay_model_providers (
 name TEXT PRIMARY KEY CHECK(name ~ '^[a-z0-9][a-z0-9_-]{0,31}$'),
 base_url TEXT CHECK(base_url IS NULL OR char_length(base_url) BETWEEN 8 AND 300),
 key_sealed BYTEA NOT NULL,
 key_hint TEXT NOT NULL CHECK(char_length(key_hint) BETWEEN 1 AND 8),
 key_rotated_ms BIGINT NOT NULL,
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL
);

-- The emergency switch, for console and file providers alike. No row: on.
-- The gateway skips a switched-off provider's routes entirely.
CREATE TABLE relay_model_provider_switches (
 provider TEXT PRIMARY KEY CHECK(char_length(provider) BETWEEN 1 AND 64),
 enabled BOOLEAN NOT NULL,
 note TEXT NOT NULL CHECK(char_length(note) <= 300),
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed_ms BIGINT NOT NULL
);

-- Platform settings an operator changes in the console. No row: the default.
CREATE TABLE relay_platform_settings (
 key TEXT PRIMARY KEY CHECK(key IN ('signup.paused','platform.daily_ceiling_usd')),
 value JSONB NOT NULL,
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed_ms BIGINT NOT NULL
);

-- Suspension refuses every member and stops agent work until lifted.
ALTER TABLE relay_workspaces
 ADD COLUMN suspended_ms BIGINT,
 ADD COLUMN suspended_note TEXT CHECK(suspended_note IS NULL OR char_length(suspended_note) BETWEEN 1 AND 300),
 ADD COLUMN suspended_by TEXT REFERENCES relay_users(id);

