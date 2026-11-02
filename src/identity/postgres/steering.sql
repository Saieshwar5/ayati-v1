-- Schema 62: steering running work and routines proposed in Chat (people-and-agents-
-- first, Phase 4 AC6, T3). What a person adds to a running request reaches the agent
-- at its next step; a routine the agent proposes is saved only when the person who
-- asked confirms it.
CREATE TABLE relay_job_steers (
 workspace TEXT NOT NULL, id TEXT NOT NULL, job TEXT NOT NULL,
 author TEXT NOT NULL REFERENCES relay_users(id),
 content TEXT NOT NULL CHECK(char_length(content) BETWEEN 1 AND 2000),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 read_ms BIGINT, carried TEXT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,job) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE INDEX relay_job_steers_job ON relay_job_steers(workspace,job,created_ms);
CREATE TABLE relay_schedule_drafts (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, job TEXT NOT NULL,
 chat TEXT NOT NULL, requester TEXT NOT NULL REFERENCES relay_users(id),
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 content TEXT NOT NULL CHECK(char_length(content) BETWEEN 1 AND 2000),
 first_ms BIGINT NOT NULL CHECK(first_ms>0),
 interval_seconds BIGINT CHECK(interval_seconds IS NULL OR interval_seconds BETWEEN 3600 AND 2592000),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','saved','dismissed')),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,job) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE INDEX relay_schedule_drafts_job ON relay_schedule_drafts(workspace,job);
