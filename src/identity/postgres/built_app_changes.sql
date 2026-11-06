-- Schema 86 (Apps): agents can act when a record changes, not only when one is added;
-- and errors an app raises while its builder previews the draft are kept for the
-- builder and the builder agent.
ALTER TABLE relay_built_app_watchers ADD COLUMN kind TEXT NOT NULL DEFAULT 'added' CHECK(kind IN ('added','changed'));
ALTER TABLE relay_built_app_watchers DROP CONSTRAINT relay_built_app_watchers_pkey;
ALTER TABLE relay_built_app_watchers ADD PRIMARY KEY(workspace,app,collection,kind);
ALTER TABLE relay_built_app_events ADD COLUMN kind TEXT NOT NULL DEFAULT 'added' CHECK(kind IN ('added','changed'));
ALTER TABLE relay_built_app_events ADD COLUMN revision BIGINT NOT NULL DEFAULT 1;
ALTER TABLE relay_built_app_events ADD COLUMN changes JSONB;
ALTER TABLE relay_built_app_events DROP CONSTRAINT relay_built_app_events_pkey;
ALTER TABLE relay_built_app_events ADD PRIMARY KEY(workspace,app,collection,record,kind,revision);
CREATE TABLE relay_built_app_preview_errors (
 workspace TEXT NOT NULL, app TEXT NOT NULL, id BIGINT GENERATED ALWAYS AS IDENTITY,
 draft_revision BIGINT NOT NULL, message TEXT NOT NULL CHECK(char_length(message) BETWEEN 1 AND 500),
 at BIGINT NOT NULL, reported_by TEXT NOT NULL REFERENCES relay_users(id),
 PRIMARY KEY(workspace,app,id),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
