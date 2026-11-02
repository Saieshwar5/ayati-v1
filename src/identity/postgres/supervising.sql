-- Schema 61: supervising at scale (people-and-agents-first, Phase 4). An agent says
-- what it needs from a person in a fixed shape (report_problem) instead of a raw
-- error; Activity → Now groups these with the platform's own signals by cause.
CREATE TABLE relay_agent_needs (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, job TEXT NOT NULL,
 cause TEXT NOT NULL CHECK(cause IN ('connection','permission','information','external','other')),
 detail TEXT NOT NULL CHECK(char_length(detail) BETWEEN 1 AND 300),
 fix TEXT CHECK(fix IS NULL OR char_length(fix) BETWEEN 1 AND 200),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 dismissed_by TEXT REFERENCES relay_users(id), dismissed_ms BIGINT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,job) REFERENCES relay_chat_jobs(workspace,reply),
 CHECK((dismissed_by IS NULL)=(dismissed_ms IS NULL))
);
CREATE INDEX relay_agent_needs_open ON relay_agent_needs(workspace,created_ms) WHERE dismissed_ms IS NULL;
