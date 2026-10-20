CREATE TABLE relay_users (
    id TEXT PRIMARY KEY, provider TEXT UNIQUE NOT NULL, email TEXT NOT NULL
);
CREATE TABLE relay_sessions (
    hash TEXT PRIMARY KEY, user_id TEXT NOT NULL REFERENCES relay_users(id),
    provider_session TEXT NOT NULL, expires BIGINT NOT NULL
);
CREATE INDEX relay_sessions_expiry ON relay_sessions(expires);
CREATE TABLE relay_workspaces (
    id TEXT PRIMARY KEY, name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
    sequence BIGINT GENERATED ALWAYS AS IDENTITY
);
CREATE TABLE relay_members (
    workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
    user_id TEXT NOT NULL REFERENCES relay_users(id),
    role TEXT NOT NULL CHECK(role IN ('Owner','Admin','Member')),
    sequence BIGINT GENERATED ALWAYS AS IDENTITY,
    PRIMARY KEY(workspace,user_id)
);
CREATE INDEX relay_members_by_user ON relay_members(user_id,workspace);
CREATE UNIQUE INDEX relay_one_owner ON relay_members(workspace) WHERE role='Owner';
CREATE TABLE relay_invitations (
    id TEXT PRIMARY KEY, workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
    email TEXT NOT NULL, role TEXT NOT NULL CHECK(role IN ('Admin','Member')),
    inviter TEXT NOT NULL REFERENCES relay_users(id), expires BIGINT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','delivery_unknown','accepted','cancelled','expired')),
    provider TEXT, sequence BIGINT GENERATED ALWAYS AS IDENTITY
);
CREATE UNIQUE INDEX relay_pending_invite ON relay_invitations(workspace,email) WHERE status='pending';
CREATE INDEX relay_invites_recipient ON relay_invitations(email,status,expires);
CREATE TABLE relay_audit (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    actor TEXT NOT NULL, workspace TEXT REFERENCES relay_workspaces(id),
    action TEXT NOT NULL, target TEXT NOT NULL, at BIGINT NOT NULL
);
CREATE TABLE relay_chats (
    id TEXT PRIMARY KEY, workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
    name TEXT NOT NULL, creator TEXT NOT NULL REFERENCES relay_users(id),
    sequence BIGINT GENERATED ALWAYS AS IDENTITY, UNIQUE(workspace,id)
);
CREATE TABLE relay_chat_people (
    workspace TEXT NOT NULL, chat TEXT NOT NULL, user_id TEXT NOT NULL,
    PRIMARY KEY(chat,user_id),
    FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
    FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_chat_agents (
    workspace TEXT NOT NULL, chat TEXT NOT NULL, agent TEXT NOT NULL,
    PRIMARY KEY(chat,agent), FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_chat_access (
    workspace TEXT NOT NULL, user_id TEXT NOT NULL, agent TEXT NOT NULL,
    PRIMARY KEY(workspace,user_id,agent),
    FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_chat_messages (
    id TEXT PRIMARY KEY, workspace TEXT NOT NULL, chat TEXT NOT NULL,
    author TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('user','agent')),
    content TEXT NOT NULL, state TEXT NOT NULL, at BIGINT NOT NULL,
    target TEXT, requester TEXT,
    sequence BIGINT GENERATED ALWAYS AS IDENTITY,
    UNIQUE(workspace,id), FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE INDEX relay_messages_by_chat ON relay_chat_messages(workspace,chat,sequence);
CREATE TABLE relay_resource_grants (
    workspace TEXT NOT NULL, user_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('app','panel')),
    resource TEXT NOT NULL, access TEXT NOT NULL CHECK(access IN ('read','write')),
    CHECK(kind='app' OR access='read'), PRIMARY KEY(workspace,user_id,kind,resource),
    FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE TABLE relay_member_app_sessions (
    workspace TEXT NOT NULL, user_id TEXT NOT NULL, app TEXT NOT NULL,
    hash TEXT PRIMARY KEY, document TEXT NOT NULL, expires BIGINT NOT NULL,
    FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_app_sessions_expiry ON relay_member_app_sessions(expires);
CREATE TABLE relay_imports (
    source_id TEXT PRIMARY KEY, completed_at BIGINT NOT NULL, counts JSONB NOT NULL
);
CREATE TABLE relay_chat_jobs (
    workspace TEXT NOT NULL, reply TEXT PRIMARY KEY, payload JSONB NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','finished')),
    claim TEXT, lease_until BIGINT NOT NULL DEFAULT 0, attempts INTEGER NOT NULL DEFAULT 0,
    sequence BIGINT GENERATED ALWAYS AS IDENTITY,
    FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id)
);
CREATE INDEX relay_jobs_ready ON relay_chat_jobs(state,lease_until);
