CREATE TABLE relay_shared_agents (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 name TEXT NOT NULL, role TEXT NOT NULL, model TEXT NOT NULL,
 PRIMARY KEY(workspace,id)
);
CREATE TABLE relay_teams (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 name TEXT NOT NULL, PRIMARY KEY(workspace,id)
);
CREATE TABLE relay_team_people (
 workspace TEXT NOT NULL, team TEXT NOT NULL, user_id TEXT NOT NULL,
 PRIMARY KEY(workspace,team,user_id),
 FOREIGN KEY(workspace,team) REFERENCES relay_teams(workspace,id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_objects (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, id TEXT NOT NULL,
 uploader TEXT NOT NULL, name TEXT NOT NULL, mime TEXT NOT NULL,
 size BIGINT NOT NULL, checksum TEXT NOT NULL, created BIGINT NOT NULL,
 PRIMARY KEY(workspace,id), FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
ALTER TABLE relay_chat_messages ADD COLUMN attachments JSONB NOT NULL DEFAULT '[]';
CREATE TABLE relay_feedback (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, id TEXT NOT NULL, reporter TEXT NOT NULL,
 title TEXT NOT NULL, expected TEXT NOT NULL, actual TEXT NOT NULL,
 category TEXT NOT NULL, severity TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'new',
 evidence TEXT NOT NULL, created BIGINT NOT NULL, revision BIGINT NOT NULL DEFAULT 0,
 PRIMARY KEY(workspace,id), FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE INDEX relay_feedback_by_chat ON relay_feedback(workspace,chat,created);
ALTER TABLE relay_chats ADD COLUMN visibility TEXT NOT NULL DEFAULT 'private' CHECK(visibility IN ('private','public'));
ALTER TABLE relay_chats ADD COLUMN announcements BOOLEAN NOT NULL DEFAULT FALSE;
CREATE TABLE relay_work_grants (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, agent TEXT NOT NULL,
 PRIMARY KEY(workspace,chat,agent), FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_company_work (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, id TEXT NOT NULL, creator TEXT NOT NULL,
 title TEXT NOT NULL, details JSONB NOT NULL, status TEXT NOT NULL DEFAULT 'draft',
 assignee TEXT NOT NULL, revision BIGINT NOT NULL DEFAULT 0, created BIGINT NOT NULL,
 PRIMARY KEY(workspace,id), FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_work_receipts (
 workspace TEXT NOT NULL, job TEXT NOT NULL, result TEXT NOT NULL,
 PRIMARY KEY(workspace,job), FOREIGN KEY(workspace,job) REFERENCES relay_chat_messages(workspace,id)
);
