-- All existing intake sources were human chat/Library requests.
ALTER TABLE relay_chat_jobs ADD COLUMN work_class TEXT NOT NULL DEFAULT 'interactive'
 CHECK(work_class IN ('interactive','background')),
 ADD COLUMN request_message TEXT UNIQUE REFERENCES relay_chat_messages(id),
 ADD CONSTRAINT relay_background_pooled CHECK(work_class<>'background' OR dispatch='pooled');
CREATE INDEX relay_jobs_class_queue ON relay_chat_jobs(work_class,sequence) WHERE state='queued';
CREATE TABLE relay_pool_turns (
 provider TEXT NOT NULL,
 model TEXT NOT NULL,
 interactive_streak SMALLINT NOT NULL CHECK(interactive_streak BETWEEN 0 AND 3),
 PRIMARY KEY(provider,model)
);
CREATE FUNCTION relay_keep_request_class() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.work_class IS DISTINCT FROM OLD.work_class OR NEW.request_message IS DISTINCT FROM OLD.request_message THEN
  RAISE EXCEPTION 'Accepted request classification is immutable';
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER relay_request_class BEFORE UPDATE OF work_class,request_message ON relay_chat_jobs
 FOR EACH ROW EXECUTE FUNCTION relay_keep_request_class();
