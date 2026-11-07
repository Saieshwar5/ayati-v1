-- Schema 100: what agents can read in a chat file (object_contents.rs).
-- One row per non-audio upload, updated in place. The bytes stay in file
-- storage; this keeps only the text a local reader found in them. The row
-- belongs to its file and goes with it.
CREATE TABLE relay_object_contents (
 workspace TEXT NOT NULL, object TEXT NOT NULL,
 status TEXT NOT NULL DEFAULT 'pending'
  CHECK(status IN ('pending','ready','unsupported','failed')),
 kind TEXT CHECK(kind IN ('text','pdf','word','excel','image')),
 text TEXT CHECK(char_length(text)<=64000),
 truncated BOOLEAN NOT NULL DEFAULT false,
 attempts SMALLINT NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 3),
 claim TEXT, lease_until_ms BIGINT NOT NULL DEFAULT 0,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 finished_ms BIGINT,
 PRIMARY KEY(workspace,object),
 FOREIGN KEY(workspace,object) REFERENCES relay_objects(workspace,id) ON DELETE CASCADE,
 CHECK((status='ready')=(text IS NOT NULL))
);
-- The queue, a deliberate scan across workspaces like the transcript queue's.
CREATE INDEX relay_object_contents_pending ON relay_object_contents(created_ms)
 WHERE status='pending';

-- A queued file wakes any controller's reader; a finished one releases the
-- request that waited for it (pool_candidate.sql).
CREATE TRIGGER relay_reading_added_work AFTER INSERT ON relay_object_contents
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('read');
CREATE TRIGGER relay_reading_done_work AFTER UPDATE OF status ON relay_object_contents
 FOR EACH ROW WHEN (OLD.status='pending' AND NEW.status<>'pending')
 EXECUTE FUNCTION relay_announce_work('dispatch');

-- Files already shared in sent messages are read too, so agents can use what
-- people sent before this schema. Unsent uploads and audio are left out.
INSERT INTO relay_object_contents(workspace,object)
SELECT DISTINCT o.workspace,o.id FROM relay_chat_messages m
 CROSS JOIN LATERAL jsonb_array_elements_text(m.attachments) AS attached(object)
 JOIN relay_objects o ON o.workspace=m.workspace AND o.chat=m.chat AND o.id=attached.object
 WHERE m.attachments<>'[]'::jsonb AND o.mime NOT LIKE 'audio/%';
