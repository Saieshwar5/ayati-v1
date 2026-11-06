-- Schema 87: Cuckoo Control, the operator console (runtime/cuckoo-control.md).

-- Every change an operator makes, from the web console or `ayati admin`.
-- Append-only: the runtime role may not update or delete rows.
CREATE TABLE relay_operator_log (
 id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
 actor TEXT NOT NULL REFERENCES relay_users(id),
 via TEXT NOT NULL CHECK(via IN ('web','cli')),
 area TEXT NOT NULL CHECK(area IN ('workspaces','providers','models','routing','settings','codes')),
 action TEXT NOT NULL CHECK(char_length(action) BETWEEN 1 AND 64),
 target TEXT NOT NULL CHECK(char_length(target) BETWEEN 1 AND 200),
 summary TEXT NOT NULL CHECK(char_length(summary) BETWEEN 1 AND 1000),
 reason TEXT NOT NULL CHECK(char_length(reason) BETWEEN 1 AND 300),
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint
);

-- Model providers added in the console. The API key is sealed with the
-- deployment's AYATI_SECRETS_KEY_FILE (AES-256-GCM); only its last four
-- characters are kept readable. Providers from AYATI_MODEL_PROVIDERS_FILE
-- are not copied here.
CREATE TABLE relay_model_providers (
 name TEXT PRIMARY KEY CHECK(name ~ '^[a-z0-9][a-z0-9_-]{0,31}$'),
 base_url TEXT NOT NULL CHECK(char_length(base_url) BETWEEN 8 AND 300),
 key_sealed BYTEA NOT NULL,
 key_hint TEXT NOT NULL CHECK(char_length(key_hint) BETWEEN 1 AND 8),
 key_rotated_ms BIGINT NOT NULL,
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL
);

-- The emergency switch, for console and file providers alike. No row: on.
CREATE TABLE relay_model_provider_switches (
 provider TEXT PRIMARY KEY CHECK(char_length(provider) BETWEEN 1 AND 64),
 enabled BOOLEAN NOT NULL,
 note TEXT NOT NULL CHECK(char_length(note) <= 300),
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed_ms BIGINT NOT NULL
);

-- Models Cuckoo may call, with prices in micro-dollars per million tokens.
-- A model with no row may still be used; its calls are recorded unpriced.
CREATE TABLE relay_platform_models (
 provider TEXT NOT NULL CHECK(char_length(provider) BETWEEN 1 AND 64),
 model TEXT NOT NULL CHECK(char_length(model) BETWEEN 1 AND 200),
 enabled BOOLEAN NOT NULL,
 input_micros_per_mtok BIGINT NOT NULL CHECK(input_micros_per_mtok BETWEEN 0 AND 1000000000),
 output_micros_per_mtok BIGINT NOT NULL CHECK(output_micros_per_mtok BETWEEN 0 AND 1000000000),
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed_ms BIGINT NOT NULL,
 PRIMARY KEY(provider,model)
);

-- How the gateway serves a model an agent asks for: a main provider and model,
-- an optional second one that takes (100 - main_percent)% of requests and every
-- request the main one fails. No row: the assignment's own provider and model.
CREATE TABLE relay_model_routes (
 model TEXT PRIMARY KEY CHECK(char_length(model) BETWEEN 1 AND 200),
 main_provider TEXT NOT NULL CHECK(char_length(main_provider) BETWEEN 1 AND 64),
 main_model TEXT NOT NULL CHECK(char_length(main_model) BETWEEN 1 AND 200),
 second_provider TEXT CHECK(char_length(second_provider) BETWEEN 1 AND 64),
 second_model TEXT CHECK(char_length(second_model) BETWEEN 1 AND 200),
 main_percent INT NOT NULL CHECK(main_percent BETWEEN 0 AND 100),
 timeout_seconds INT NOT NULL CHECK(timeout_seconds BETWEEN 5 AND 600),
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed_ms BIGINT NOT NULL,
 CHECK((second_provider IS NULL) = (second_model IS NULL)),
 CHECK(second_provider IS NOT NULL OR main_percent = 100)
);

-- Platform settings an operator changes in the console. No row: the default.
CREATE TABLE relay_platform_settings (
 key TEXT PRIMARY KEY CHECK(key IN ('signup.paused','workspace.default_cap_usd','platform.daily_ceiling_usd')),
 value JSONB NOT NULL,
 changed_by TEXT NOT NULL REFERENCES relay_users(id),
 changed_ms BIGINT NOT NULL
);

-- Suspension refuses every member and stops agent work; the cap limits a
-- workspace's model spend per calendar month (UTC). NULL cap: the default.
ALTER TABLE relay_workspaces
 ADD COLUMN suspended_ms BIGINT,
 ADD COLUMN suspended_note TEXT CHECK(suspended_note IS NULL OR char_length(suspended_note) BETWEEN 1 AND 300),
 ADD COLUMN suspended_by TEXT REFERENCES relay_users(id),
 ADD COLUMN spend_cap_micros BIGINT CHECK(spend_cap_micros IS NULL OR spend_cap_micros >= 0);

-- Cost at the price when the call was made, how long it took, and whether it
-- failed. Failed calls are now recorded too (with no tokens), so the console
-- can show error rates. Earlier rows have no cost or latency.
ALTER TABLE relay_model_usage
 ADD COLUMN cost_micros BIGINT CHECK(cost_micros IS NULL OR cost_micros >= 0),
 ADD COLUMN latency_ms INT CHECK(latency_ms IS NULL OR latency_ms >= 0),
 ADD COLUMN outcome TEXT NOT NULL DEFAULT 'ok' CHECK(outcome IN ('ok','error')),
 ADD COLUMN fell_back BOOLEAN NOT NULL DEFAULT false,
 ADD COLUMN requested_model TEXT CHECK(requested_model IS NULL OR char_length(requested_model) BETWEEN 1 AND 200);
CREATE INDEX relay_model_usage_time ON relay_model_usage(at_ms);
CREATE INDEX relay_model_usage_workspace_time ON relay_model_usage(workspace,at_ms);
