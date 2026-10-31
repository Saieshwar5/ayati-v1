-- Skills (D234): how-to packages in the Agent Skills format. Agents read their
-- assigned skills through ayati_control (load_skill); nothing is written to
-- executors and Goose's built-in skills never appear. Scripts are stored as text
-- and never run.
CREATE TABLE relay_skills (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 id TEXT NOT NULL,
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 64 AND name ~ '^[a-z0-9]+(-[a-z0-9]+)*$'),
 description TEXT NOT NULL CHECK(length(description) BETWEEN 1 AND 1024),
 owner_id TEXT NOT NULL,
 -- Workspace skills are shared and changed only by admins; others are "My skills".
 shared BOOLEAN NOT NULL DEFAULT FALSE,
 version INTEGER NOT NULL DEFAULT 1 CHECK(version > 0),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id)
);
-- An agent loads a skill by name, so names are unique in a workspace.
CREATE UNIQUE INDEX relay_skills_name ON relay_skills(workspace,name);
CREATE TABLE relay_skill_versions (
 workspace TEXT NOT NULL, skill TEXT NOT NULL, version INTEGER NOT NULL,
 hash TEXT NOT NULL CHECK(length(hash) = 64),
 description TEXT NOT NULL,
 license TEXT, has_scripts BOOLEAN NOT NULL,
 -- [{path, text}], validated on upload: text only, at most 100 files and 5 MB.
 files JSONB NOT NULL CHECK(jsonb_typeof(files) = 'array'),
 author TEXT NOT NULL,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,skill,version),
 FOREIGN KEY(workspace,skill) REFERENCES relay_skills(workspace,id) ON DELETE CASCADE
);
CREATE TABLE relay_agent_skills (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, skill TEXT NOT NULL,
 added_by TEXT NOT NULL,
 added_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,agent,skill),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,skill) REFERENCES relay_skills(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_agent_skills_skill ON relay_agent_skills(workspace,skill);
-- Each load_skill call, shown on the request in Activity.
CREATE TABLE relay_skill_loads (
 workspace TEXT NOT NULL, job TEXT NOT NULL, skill TEXT NOT NULL, version INTEGER NOT NULL,
 path TEXT,
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint
);
CREATE INDEX relay_skill_loads_job ON relay_skill_loads(workspace,job);
CREATE INDEX relay_skill_loads_skill ON relay_skill_loads(workspace,skill,at_ms DESC);
