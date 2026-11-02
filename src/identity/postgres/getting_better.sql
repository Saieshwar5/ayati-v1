-- Schema 63: getting better over time (people-and-agents-first, Phase 5). Goals per
-- agent with progress the agent reports, weekly reviews agents write for their
-- managers, and repeated approvals that people chose not to turn into rules.
CREATE TABLE relay_agent_goals (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL,
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 target DOUBLE PRECISION NOT NULL CHECK(target>0),
 unit TEXT NOT NULL DEFAULT '' CHECK(char_length(unit)<=16),
 period TEXT NOT NULL CHECK(period IN ('week','month')),
 current DOUBLE PRECISION, risk TEXT CHECK(risk IS NULL OR char_length(risk) BETWEEN 1 AND 280),
 updated_ms BIGINT, created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 archived BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id)
);
CREATE INDEX relay_agent_goals_agent ON relay_agent_goals(workspace,agent) WHERE NOT archived;
CREATE TABLE relay_agent_reviews (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, job TEXT NOT NULL,
 done TEXT NOT NULL CHECK(char_length(done) BETWEEN 1 AND 1000),
 went_wrong TEXT NOT NULL CHECK(char_length(went_wrong) BETWEEN 1 AND 1000),
 learned TEXT NOT NULL CHECK(char_length(learned) BETWEEN 1 AND 1000),
 suggestion TEXT CHECK(suggestion IS NULL OR char_length(suggestion) BETWEEN 1 AND 1000),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id), UNIQUE(workspace,job),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,job) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE INDEX relay_agent_reviews_agent ON relay_agent_reviews(workspace,agent,created_ms DESC);
-- The weekly review routine of each agent that has one.
CREATE TABLE relay_review_routines (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, schedule TEXT NOT NULL,
 PRIMARY KEY(workspace,agent),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,schedule) REFERENCES relay_agent_schedules(workspace,id)
);
CREATE TABLE relay_rule_suggestions_dismissed (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, connection TEXT NOT NULL, operation TEXT NOT NULL,
 user_id TEXT NOT NULL REFERENCES relay_users(id),
 at BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,agent,connection,operation,user_id)
);
