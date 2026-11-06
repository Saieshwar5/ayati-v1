-- Schema 78 (Apps AP4–AP9, first part): apps built in Cuckoo. Their files are kept per
-- version here (the code store's first adapter): version 0 is the draft, every publish
-- is a new numbered version, and the published one is what people open.
CREATE TABLE relay_built_apps (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 description TEXT NOT NULL DEFAULT '' CHECK(char_length(description)<=300),
 builder TEXT NOT NULL REFERENCES relay_users(id),
 audience TEXT NOT NULL CHECK(audience IN ('everyone','selected')),
 published INTEGER, draft_revision BIGINT NOT NULL DEFAULT 1,
 created BIGINT NOT NULL, updated BIGINT NOT NULL, removed BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(workspace,id)
);
CREATE TABLE relay_built_app_people (
 workspace TEXT NOT NULL, app TEXT NOT NULL, user_id TEXT NOT NULL,
 PRIMARY KEY(workspace,app,user_id),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_built_app_files (
 workspace TEXT NOT NULL, app TEXT NOT NULL, version INTEGER NOT NULL CHECK(version>=0),
 path TEXT NOT NULL, content TEXT NOT NULL CHECK(octet_length(content)<=262144),
 PRIMARY KEY(workspace,app,version,path),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
CREATE TABLE relay_built_app_versions (
 workspace TEXT NOT NULL, app TEXT NOT NULL, version INTEGER NOT NULL CHECK(version>=1),
 note TEXT NOT NULL DEFAULT '' CHECK(char_length(note)<=300),
 published_by TEXT NOT NULL REFERENCES relay_users(id), published_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,app,version),
 FOREIGN KEY(workspace,app) REFERENCES relay_built_apps(workspace,id) ON DELETE CASCADE
);
