-- Schema 121: agents learn from their work (knowledge K1). Cuckoo proposes
-- lessons from shared conversations; nothing reaches an agent until a person
-- who maintains it approves the lesson, which then becomes an ordinary note.
CREATE TABLE relay_agent_learning (
 workspace TEXT NOT NULL, agent TEXT NOT NULL,
 enabled BOOLEAN NOT NULL DEFAULT true,
 -- Work up to this time has been read; the next run starts after it.
 learned_until_ms BIGINT NOT NULL DEFAULT 0,
 last_run_ms BIGINT, last_result TEXT CHECK(last_result IS NULL OR char_length(last_result)<=200),
 revision BIGINT NOT NULL DEFAULT 0,
 PRIMARY KEY(workspace,agent), FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id)
);
CREATE TABLE relay_knowledge_proposals (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, id TEXT NOT NULL,
 -- The shared conversation every piece of evidence came from, if only one.
 chat TEXT,
 title TEXT NOT NULL CHECK(char_length(title) BETWEEN 1 AND 160),
 content TEXT NOT NULL CHECK(char_length(content) BETWEEN 1 AND 2000),
 why TEXT NOT NULL CHECK(char_length(why)<=1000),
 evidence JSONB NOT NULL CHECK(jsonb_typeof(evidence)='array' AND jsonb_array_length(evidence) BETWEEN 1 AND 6),
 kind TEXT NOT NULL CHECK(kind IN ('new','update','conflict')),
 -- The existing note an update or conflict is about.
 replaces TEXT,
 state TEXT NOT NULL DEFAULT 'proposed' CHECK(state IN ('proposed','approved','rejected')),
 note TEXT, model TEXT NOT NULL CHECK(char_length(model) BETWEEN 1 AND 200),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 decided_by TEXT REFERENCES relay_users(id), decided_ms BIGINT,
 PRIMARY KEY(workspace,agent,id), FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE INDEX relay_knowledge_proposals_recent ON relay_knowledge_proposals(workspace,agent,created_ms DESC);
-- An approved lesson's note says where it came from.
ALTER TABLE relay_agent_knowledge ADD COLUMN learned_from TEXT;
