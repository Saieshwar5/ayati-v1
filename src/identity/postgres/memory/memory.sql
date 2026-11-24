-- Schema 142: one memory engine with four kinds of shelf. The workspace's
-- (facts about the business, every agent), an agent's job (what it learned
-- about its own work), one agent in one room (a channel, group or work room)
-- and one agent with one person (their direct chat). Which shelves a request
-- reads and writes is decided by the controller from the running task.
CREATE FUNCTION relay_memory_half_life_ms(kind TEXT) RETURNS DOUBLE PRECISION
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT 86400000.0 * CASE kind WHEN 'status' THEN 14 WHEN 'people' THEN 90
    WHEN 'customer' THEN 180 WHEN 'system' THEN 180 WHEN 'preference' THEN 180 ELSE 365 END
$$;
-- A note's strength now: unused, it halves once per half-life of its kind.
-- Pinned notes never fade.
CREATE FUNCTION relay_memory_strength(strength INTEGER, kind TEXT, used_ms BIGINT, pinned BOOLEAN, now_ms BIGINT)
RETURNS INTEGER LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
  SELECT CASE WHEN pinned THEN strength
    ELSE round(strength * power(0.5, greatest(0, now_ms - used_ms) / relay_memory_half_life_ms(kind)))::integer END
$$;

CREATE TABLE relay_memories (
 workspace TEXT NOT NULL, id TEXT NOT NULL,
 scope TEXT NOT NULL CHECK(scope IN ('workspace','job','room','person')),
 agent TEXT, chat TEXT, user_id TEXT,
 shelf TEXT GENERATED ALWAYS AS (scope||':'||COALESCE(agent,'')||':'||COALESCE(chat,'')||':'||COALESCE(user_id,'')) STORED,
 kind TEXT NOT NULL CHECK(kind IN ('customer','system','people','decision','practice','preference','status','term')),
 topic TEXT NOT NULL DEFAULT '' CHECK(char_length(topic)<=80),
 text TEXT NOT NULL CHECK(char_length(text) BETWEEN 1 AND 300),
 -- Who wrote it: an agent while working, a person on their own shelf, an
 -- admin or owner, or the tidy-up (a merged note).
 source TEXT NOT NULL CHECK(source IN ('agent','person','admin','tidy')),
 author TEXT,
 -- The request it was kept in and the conversation it was learned in.
 job TEXT, learned_in TEXT,
 -- active: used. waiting: a person decides first. ended: stopped being true.
 -- archived: faded away, kept for a while. folded: written into its topic's summary.
 state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','waiting','ended','archived','folded')),
 waiting_reason TEXT CHECK(waiting_reason IN ('private_source','outside','background','disagreement')),
 strength INTEGER NOT NULL CHECK(strength BETWEEN 0 AND 100),
 pinned BOOLEAN NOT NULL DEFAULT false,
 essential BOOLEAN NOT NULL DEFAULT false,
 used INTEGER NOT NULL DEFAULT 0,
 used_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 until_ms BIGINT,
 replaces TEXT, replaced_by TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 changed_ms BIGINT,
 PRIMARY KEY(workspace,id),
 CHECK((scope='workspace' AND agent IS NULL AND chat IS NULL AND user_id IS NULL)
    OR (scope='job' AND agent IS NOT NULL AND chat IS NULL AND user_id IS NULL)
    OR (scope='room' AND agent IS NOT NULL AND chat IS NOT NULL AND user_id IS NULL)
    OR (scope='person' AND agent IS NOT NULL AND chat IS NULL AND user_id IS NOT NULL)),
 CHECK((state='waiting')=(waiting_reason IS NOT NULL)),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_memories_shelf ON relay_memories(workspace,shelf,state,created_ms DESC);
CREATE INDEX relay_memories_words ON relay_memories USING gin(to_tsvector('simple',topic||' '||text)) WHERE state='active';

-- One summary per topic per shelf, written by the tidy-up or a person.
CREATE TABLE relay_memory_topics (
 workspace TEXT NOT NULL,
 scope TEXT NOT NULL CHECK(scope IN ('workspace','job','room','person')),
 agent TEXT, chat TEXT, user_id TEXT,
 shelf TEXT GENERATED ALWAYS AS (scope||':'||COALESCE(agent,'')||':'||COALESCE(chat,'')||':'||COALESCE(user_id,'')) STORED,
 topic TEXT NOT NULL CHECK(char_length(topic) BETWEEN 1 AND 80),
 summary TEXT NOT NULL CHECK(char_length(summary) BETWEEN 1 AND 1200),
 source TEXT NOT NULL CHECK(source IN ('tidy','admin')),
 updated_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,shelf,topic),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);

-- Every memory action on a request, as the server did it: shown in Activity,
-- and the source of "looked for, not found".
CREATE TABLE relay_memory_uses (
 workspace TEXT NOT NULL, job TEXT NOT NULL, agent TEXT NOT NULL, chat TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('given','searched','searched_history','kept','suggested','ended')),
 query TEXT, found INTEGER, memory TEXT,
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_memory_uses_job ON relay_memory_uses(workspace,job,at_ms);
CREATE UNIQUE INDEX relay_memory_uses_given ON relay_memory_uses(workspace,job) WHERE action='given';
CREATE INDEX relay_memory_uses_missed ON relay_memory_uses(workspace,at_ms DESC) WHERE action='searched' AND found=0;

-- What the tidy-up changed, with the rows as they were, so a person can undo it.
CREATE TABLE relay_memory_changes (
 workspace TEXT NOT NULL, id TEXT NOT NULL,
 scope TEXT NOT NULL, agent TEXT, user_id TEXT, shelf TEXT NOT NULL,
 action TEXT NOT NULL CHECK(action IN ('expired','merged','archived','deleted','summarised')),
 summary TEXT NOT NULL,
 before JSONB NOT NULL,
 at_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 undone_ms BIGINT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_memory_changes_shelf ON relay_memory_changes(workspace,scope,agent,at_ms DESC);

-- When each workspace's memory was last tidied.
CREATE TABLE relay_memory_tidy (
 workspace TEXT PRIMARY KEY REFERENCES relay_workspaces(id) ON DELETE CASCADE,
 last_run_ms BIGINT NOT NULL
);
