ALTER TABLE relay_shared_agents
 ADD COLUMN scope TEXT NOT NULL DEFAULT 'workspace' CHECK(scope IN ('private','workspace')),
 ADD COLUMN owner_id TEXT REFERENCES relay_users(id),
 ADD COLUMN creator_id TEXT REFERENCES relay_users(id),
 ADD COLUMN created_at BIGINT NOT NULL DEFAULT extract(epoch FROM now())::bigint,
 ADD COLUMN revision BIGINT NOT NULL DEFAULT 0,
 ADD COLUMN settings JSONB NOT NULL DEFAULT '{}'::jsonb;
UPDATE relay_shared_agents a SET owner_id=m.user_id FROM relay_members m WHERE m.workspace=a.workspace AND m.role='Owner';
ALTER TABLE relay_chat_agents ADD COLUMN joined_sequence BIGINT NOT NULL DEFAULT 0;
CREATE TABLE relay_agent_avatars (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, id TEXT NOT NULL, mime TEXT NOT NULL, bytes BYTEA NOT NULL,
 PRIMARY KEY(workspace,agent,id), FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id)
);
CREATE TABLE relay_agent_knowledge (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, id TEXT NOT NULL, chat TEXT,
 title TEXT NOT NULL, content TEXT NOT NULL, source_message TEXT, creator_id TEXT NOT NULL REFERENCES relay_users(id),
 revision BIGINT NOT NULL DEFAULT 0, forgotten BOOLEAN NOT NULL DEFAULT false, created_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,agent,id), FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE INDEX relay_agent_knowledge_scope ON relay_agent_knowledge(workspace,agent,chat) WHERE NOT forgotten;
