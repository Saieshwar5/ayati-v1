-- Schema 120: working summaries Cuckoo keeps up to date itself when a
-- conversation grows long. They sit beside the agent's own saved summaries,
-- in the same room-scoped versions, and record what they covered.
ALTER TABLE relay_working_context
 ADD COLUMN made_by TEXT NOT NULL DEFAULT 'agent' CHECK(made_by IN ('agent','automatic')),
 ADD COLUMN model TEXT CHECK(model IS NULL OR char_length(model) BETWEEN 1 AND 200),
 ADD COLUMN covered JSONB CHECK(covered IS NULL OR (jsonb_typeof(covered)='array' AND jsonb_array_length(covered)<=200));

-- Saved context stays immutable. Forgetting (a connected system's subject) is
-- the one deliberate removal, and only in a transaction that says so.
CREATE OR REPLACE FUNCTION relay_context_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='DELETE' AND current_setting('relay.forget_context', true)='on' THEN RETURN OLD; END IF;
 RAISE EXCEPTION 'Saved context evidence is immutable';
END $$;

-- The background summarizer reads finished work in order.
CREATE INDEX relay_chat_jobs_finished ON relay_chat_jobs(finished_ms,reply) WHERE state='finished';
