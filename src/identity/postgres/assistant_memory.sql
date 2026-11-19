-- Schema 138: an assistant's long-term memory.
--
-- One row per thing a member's assistant remembers about them and their work.
-- Rows are added, never rewritten by the assistant: when one stops being true
-- the assistant ends it (ended_ms) and adds the new one, so its person can see
-- what changed and bring a line back. Only its person reads these.
CREATE TABLE relay_assistant_memories (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL, id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('about','style','ways','people','work')),
 text TEXT NOT NULL CHECK(char_length(text) BETWEEN 1 AND 300),
 -- Who wrote it: the person on its page, the assistant while working with
 -- them (chat), or the assistant looking back at their work (review).
 source TEXT NOT NULL CHECK(source IN ('you','chat','review')),
 -- The reply the assistant was writing when it kept or ended this.
 origin TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 ended_ms BIGINT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_assistant_memories_person ON relay_assistant_memories(workspace,user_id,created_ms DESC) WHERE ended_ms IS NULL;
CREATE INDEX relay_assistant_memories_words ON relay_assistant_memories USING gin(to_tsvector('simple',text)) WHERE ended_ms IS NULL;

-- The two note texts of schema 129 become memories, one per line.
INSERT INTO relay_assistant_memories(workspace,user_id,id,kind,text,source,created_ms)
SELECT s.workspace,s.user_id,gen_random_uuid()::text,k.kind,left(btrim(l.line),300),'you',s.updated_ms+l.at
FROM relay_assistant_settings s
CROSS JOIN LATERAL (VALUES('about',s.about),('style',s.style)) k(kind,body)
CROSS JOIN LATERAL unnest(string_to_array(k.body,E'\n')) WITH ORDINALITY l(line,at)
WHERE btrim(l.line)<>'';

-- Looking back at the day's work: the newest message already looked at, and
-- when to look next.
ALTER TABLE relay_assistant_settings
 ADD COLUMN reflected BIGINT NOT NULL DEFAULT 0,
 ADD COLUMN reflect_next_ms BIGINT,
 ADD COLUMN reflect_job TEXT;
