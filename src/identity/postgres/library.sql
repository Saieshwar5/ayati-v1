CREATE TABLE relay_library_items (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('page','file')), title TEXT NOT NULL,
 created_by TEXT NOT NULL REFERENCES relay_users(id), steward TEXT,
 visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('private','selected','workspace')),
 revision BIGINT NOT NULL DEFAULT 1, access_revision BIGINT NOT NULL DEFAULT 1,
 document JSONB, text TEXT NOT NULL DEFAULT '', object_key TEXT,
 mime TEXT NOT NULL DEFAULT '', size BIGINT NOT NULL DEFAULT 0, checksum TEXT NOT NULL DEFAULT '',
 readiness TEXT NOT NULL DEFAULT 'ready', created BIGINT NOT NULL, updated BIGINT NOT NULL,
 trashed BOOLEAN NOT NULL DEFAULT false, PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,steward) REFERENCES relay_members(workspace,user_id) ON DELETE SET NULL (steward)
);
CREATE INDEX relay_library_browse ON relay_library_items(workspace,updated DESC,id);
CREATE TABLE relay_library_people (
 workspace TEXT NOT NULL, item TEXT NOT NULL, user_id TEXT NOT NULL,
 access TEXT NOT NULL CHECK(access IN ('view','edit')), PRIMARY KEY(workspace,item,user_id),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_library_agents (
 workspace TEXT NOT NULL, item TEXT NOT NULL, agent TEXT NOT NULL,
 PRIMARY KEY(workspace,item,agent),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE TABLE relay_library_versions (
 workspace TEXT NOT NULL, item TEXT NOT NULL, revision BIGINT NOT NULL,
 author TEXT NOT NULL REFERENCES relay_users(id), title TEXT NOT NULL,
 document JSONB NOT NULL, text TEXT NOT NULL, created BIGINT NOT NULL,
 PRIMARY KEY(workspace,item,revision),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id)
);
CREATE TABLE relay_library_stars (
 workspace TEXT NOT NULL, item TEXT NOT NULL, user_id TEXT NOT NULL,
 PRIMARY KEY(workspace,item,user_id),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_library_requests (
 workspace TEXT NOT NULL, actor TEXT NOT NULL, id TEXT NOT NULL,
 payload JSONB NOT NULL, result JSONB NOT NULL,
 PRIMARY KEY(workspace,actor,id),
 FOREIGN KEY(workspace,actor) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_library_uploads (
 workspace TEXT NOT NULL, id TEXT NOT NULL, actor TEXT NOT NULL,
 title TEXT NOT NULL, size BIGINT NOT NULL, checksum TEXT NOT NULL,
 object_key TEXT NOT NULL, expires BIGINT NOT NULL, state TEXT NOT NULL DEFAULT 'pending',
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,actor) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_library_sources (
 workspace TEXT NOT NULL, reply TEXT NOT NULL, item TEXT NOT NULL,
 revision BIGINT NOT NULL, access_revision BIGINT NOT NULL,
 PRIMARY KEY(workspace,reply,item),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id)
);
