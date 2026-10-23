-- Drafts are separate from authoritative completed message content.
ALTER TABLE relay_chat_messages ADD COLUMN progress JSONB,
 ADD COLUMN revision BIGINT NOT NULL DEFAULT 0;
ALTER TABLE relay_assignment_credentials ADD COLUMN output_sequence BIGINT NOT NULL DEFAULT 0 CHECK(output_sequence BETWEEN 0 AND 4096),
 ADD COLUMN output_bytes INTEGER NOT NULL DEFAULT 0 CHECK(output_bytes BETWEEN 0 AND 65536);
ALTER TABLE relay_worker_assignments ADD COLUMN context_ready_ms BIGINT,
 ADD COLUMN prompt_reported_ms BIGINT, ADD COLUMN first_output_ms BIGINT;
CREATE TABLE relay_assignment_output (
 assignment TEXT NOT NULL REFERENCES relay_worker_assignments(id),
 sequence BIGINT NOT NULL CHECK(sequence BETWEEN 1 AND 4096),
 fingerprint TEXT NOT NULL CHECK(length(fingerprint)=43),
 phase TEXT NOT NULL CHECK(phase IN ('waiting','responding')),
 text TEXT NOT NULL CHECK(octet_length(text)<=4096),
 received_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(assignment,sequence)
);
CREATE FUNCTION relay_reply_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.state<>'queued' THEN NEW.progress := NULL; END IF;
 NEW.revision := OLD.revision+1;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_reply_revision BEFORE UPDATE ON relay_chat_messages
 FOR EACH ROW EXECUTE FUNCTION relay_reply_revision();
CREATE FUNCTION relay_reply_lifecycle() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE phase text;
BEGIN
 IF TG_OP='INSERT' THEN phase := 'starting';
 ELSIF NEW.state=OLD.state THEN RETURN NEW;
 ELSIF NEW.state='running' THEN phase := 'starting';
 ELSIF NEW.state='lost' THEN phase := 'interrupted';
 ELSE RETURN NEW;
 END IF;
 UPDATE relay_chat_messages SET progress=jsonb_build_object('state',phase,
  'generation',NEW.generation,'text',CASE WHEN phase='interrupted'
    THEN COALESCE(progress->>'text','') ELSE '' END)
 WHERE workspace=NEW.workspace AND id=NEW.reply AND state='queued';
 RETURN NEW;
END $$;
CREATE TRIGGER relay_reply_lifecycle AFTER INSERT OR UPDATE ON relay_worker_assignments
 FOR EACH ROW EXECUTE FUNCTION relay_reply_lifecycle();
