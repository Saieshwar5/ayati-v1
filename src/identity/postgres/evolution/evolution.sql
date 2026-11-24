-- Schema 143: agents that get better on their own. Every answer gets an outcome;
-- the notes a request used are kept so outcomes can change them; requests are
-- sorted into each agent's job types; every three days an agent reviews its
-- routine work and proposes skills, which a person approves.

-- How each answer landed: from the asker's next message, reactions and refused approvals.
CREATE TABLE relay_request_outcomes (
 workspace TEXT NOT NULL, job TEXT NOT NULL, agent TEXT NOT NULL, chat TEXT NOT NULL,
 outcome TEXT NOT NULL CHECK(outcome IN ('accepted','corrected','followed_up','unknown')),
 confidence REAL NOT NULL CHECK(confidence BETWEEN 0 AND 1),
 -- What decided it: the classifier, a reaction, a refused approval, or time passing.
 source TEXT NOT NULL CHECK(source IN ('classifier','reaction','approval','time')),
 -- The message that showed it, when one did.
 message TEXT,
 model TEXT,
 decided_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,job),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_request_outcomes_agent ON relay_request_outcomes(workspace,agent,decided_ms DESC);

-- The notes a request was given or found, so its outcome can change them.
CREATE TABLE relay_memory_used (
 workspace TEXT NOT NULL, job TEXT NOT NULL, memory TEXT NOT NULL,
 how TEXT NOT NULL CHECK(how IN ('given','found')),
 settled BOOLEAN NOT NULL DEFAULT false,
 PRIMARY KEY(workspace,job,memory),
 FOREIGN KEY(workspace,memory) REFERENCES relay_memories(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_memory_used_memory ON relay_memory_used(workspace,memory);

-- A note waits for a person when an answer that used it was corrected, and is
-- checked by the classifier once after it is kept.
ALTER TABLE relay_memories DROP CONSTRAINT relay_memories_waiting_reason_check;
ALTER TABLE relay_memories ADD CONSTRAINT relay_memories_waiting_reason_check
 CHECK(waiting_reason IN ('private_source','outside','background','disagreement','corrected'));
ALTER TABLE relay_memories ADD COLUMN checked_ms BIGINT, ADD COLUMN check_result JSONB;
-- Notes kept before the classifier existed are not checked again.
UPDATE relay_memories SET checked_ms=created_ms;
CREATE INDEX relay_memories_unchecked ON relay_memories(workspace,created_ms) WHERE checked_ms IS NULL;

-- The kinds of work each agent does, named from its requests.
CREATE TABLE relay_job_types (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL,
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 description TEXT NOT NULL DEFAULT '' CHECK(char_length(description)<=300),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE UNIQUE INDEX relay_job_types_name ON relay_job_types(workspace,agent,lower(name));
CREATE TABLE relay_request_job_types (
 workspace TEXT NOT NULL, job TEXT NOT NULL, agent TEXT NOT NULL, job_type TEXT NOT NULL,
 confidence REAL NOT NULL CHECK(confidence BETWEEN 0 AND 1),
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,job),
 FOREIGN KEY(workspace,job_type) REFERENCES relay_job_types(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_request_job_types_type ON relay_request_job_types(workspace,job_type,at_ms DESC);

-- Every three-day review an agent made of its work, including those that changed nothing.
CREATE TABLE relay_skill_reviews (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL,
 ran_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 requests INTEGER NOT NULL, job_types INTEGER NOT NULL, routine INTEGER NOT NULL, proposed INTEGER NOT NULL,
 result TEXT NOT NULL CHECK(char_length(result) BETWEEN 1 AND 400),
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_skill_reviews_agent ON relay_skill_reviews(workspace,agent,ran_ms DESC);

-- A skill an agent proposes: new, a new version of one it has, or retiring one.
CREATE TABLE relay_skill_proposals (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, review TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('new','improve','retire')),
 job_type TEXT, skill TEXT,
 name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '',
 -- [{path, text}] for new and improve; empty for retire.
 files JSONB NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(files)='array'),
 why TEXT NOT NULL CHECK(char_length(why) BETWEEN 1 AND 600),
 -- [{job, chat, at_ms, outcome}]: the requests it came from.
 evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
 -- [{job, verdict, note}]: the draft tried on earlier requests.
 rehearsals JSONB NOT NULL DEFAULT '[]'::jsonb,
 state TEXT NOT NULL DEFAULT 'proposed' CHECK(state IN ('proposed','approved','rejected')),
 decided_by TEXT, decided_ms BIGINT, created_skill TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_skill_proposals_agent ON relay_skill_proposals(workspace,agent,created_ms DESC);

-- Skills and versions an agent wrote say which agent.
ALTER TABLE relay_skills ADD COLUMN created_by_agent TEXT;
ALTER TABLE relay_skill_versions ADD COLUMN agent TEXT;

-- Notes' meaning, where pgvector is installed (as the Library's passages).
DO $$
BEGIN
 IF EXISTS(SELECT 1 FROM pg_available_extensions WHERE name='vector') THEN
  PERFORM pg_advisory_xact_lock(hashtext('relay_library_vector_extension'));
  CREATE EXTENSION IF NOT EXISTS vector SCHEMA public;
  EXECUTE format('CREATE TABLE relay_memory_embeddings (
   workspace TEXT NOT NULL, memory TEXT NOT NULL, model TEXT NOT NULL, embedding %I.vector(1536) NOT NULL,
   PRIMARY KEY(workspace,memory),
   FOREIGN KEY(workspace,memory) REFERENCES relay_memories(workspace,id) ON DELETE CASCADE)',
   (SELECT n.nspname FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='vector'));
 END IF;
END $$;
