-- Personal, tool-free model samples; never business jobs or dispatch authority.
CREATE TABLE relay_agent_rehearsals (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, actor TEXT NOT NULL,
 request_hash TEXT NOT NULL, input JSONB NOT NULL, profile JSONB NOT NULL,
 note_refs JSONB NOT NULL DEFAULT '[]',
 prepared JSONB CHECK(octet_length(prepared::text)<=262144),
 state TEXT NOT NULL DEFAULT 'queued'
  CHECK(state IN ('queued','running','completed','failed','cancelled','interrupted','invalidated')),
 claim TEXT, lease_until BIGINT NOT NULL DEFAULT 0,
 output TEXT CHECK(octet_length(output)<=65536), detail TEXT NOT NULL DEFAULT '',
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 started_ms BIGINT, finished_ms BIGINT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 -- Retain the execution receipt across membership removal/rejoin. Current
 -- membership is rechecked on reads; an old request must never rerun inference.
 FOREIGN KEY(actor) REFERENCES relay_users(id),
 CHECK((state='running')=(claim IS NOT NULL)),
 CHECK((state IN ('queued','running'))=(finished_ms IS NULL)),
 CHECK(state='completed' OR output IS NULL),
 CHECK((state IN ('queued','running'))=(prepared IS NOT NULL))
);
CREATE INDEX relay_rehearsal_queue ON relay_agent_rehearsals(created_ms,id) WHERE state='queued';
CREATE INDEX relay_rehearsal_history ON relay_agent_rehearsals(workspace,actor,agent,created_ms DESC,id DESC);
CREATE UNIQUE INDEX relay_one_active_sample ON relay_agent_rehearsals(workspace,agent,actor)
 WHERE state IN ('queued','running');
CREATE FUNCTION relay_rehearsal_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.workspace,NEW.id,NEW.agent,NEW.actor,NEW.request_hash,NEW.input,NEW.profile,NEW.note_refs,NEW.created_ms)
  IS DISTINCT FROM ROW(OLD.workspace,OLD.id,OLD.agent,OLD.actor,OLD.request_hash,OLD.input,OLD.profile,OLD.note_refs,OLD.created_ms) THEN
  RAISE EXCEPTION 'A saved sample request cannot change';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_rehearsal_immutable BEFORE UPDATE ON relay_agent_rehearsals
 FOR EACH ROW EXECUTE FUNCTION relay_rehearsal_immutable();
