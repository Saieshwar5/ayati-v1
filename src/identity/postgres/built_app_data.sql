-- Schema 79 (Apps BA2): what built apps keep, and the roles people hold in them. The
-- published app reads and writes 'live' records; its draft preview keeps 'preview'
-- records apart, so trying a draft never touches the data people rely on.
CREATE TABLE relay_built_app_roles (
 workspace TEXT NOT NULL, app TEXT NOT NULL, user_id TEXT NOT NULL,
 role TEXT NOT NULL CHECK(char_length(role) BETWEEN 1 AND 40),
 PRIMARY KEY(workspace,app,user_id,role),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_built_app_records (
 workspace TEXT NOT NULL, app TEXT NOT NULL,
 stage TEXT NOT NULL CHECK(stage IN ('live','preview')),
 collection TEXT NOT NULL CHECK(char_length(collection) BETWEEN 1 AND 40),
 id TEXT NOT NULL, data JSONB NOT NULL CHECK(jsonb_typeof(data)='object' AND octet_length(data::text)<=32768),
 revision BIGINT NOT NULL DEFAULT 1,
 created_by TEXT NOT NULL REFERENCES relay_users(id), updated_by TEXT NOT NULL REFERENCES relay_users(id),
 created BIGINT NOT NULL, updated BIGINT NOT NULL,
 PRIMARY KEY(workspace,app,stage,collection,id),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_built_app_records_recent ON relay_built_app_records(workspace,app,stage,collection,created DESC,id);
