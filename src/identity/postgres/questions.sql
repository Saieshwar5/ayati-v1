CREATE TABLE relay_task_questions (
 workspace TEXT NOT NULL, id TEXT NOT NULL, reply TEXT NOT NULL,
 request_key TEXT NOT NULL CHECK(length(request_key) BETWEEN 1 AND 64),
 request_hash TEXT NOT NULL, run_cycle INTEGER NOT NULL,
 specification JSONB NOT NULL CHECK(octet_length(specification::text)<=20000),
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision=1),
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','answered','declined','invalidated')),
 answer JSONB CHECK(octet_length(answer::text)<=10000),
 submission TEXT, submission_hash TEXT, answered_by TEXT, answered_ms BIGINT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id), UNIQUE(workspace,reply,request_key),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply),
 CHECK((state IN ('answered','declined')) = (submission IS NOT NULL)),
 CHECK((submission IS NULL) = (submission_hash IS NULL)),
 CHECK((submission IS NULL) = (answered_by IS NULL)),
 CHECK((submission IS NULL) = (answered_ms IS NULL))
);
CREATE UNIQUE INDEX relay_one_pending_question ON relay_task_questions(workspace,reply)
 WHERE state='pending';

-- Input and business-action waits compose under the same task transaction.
CREATE OR REPLACE FUNCTION relay_resume_waiting_task(w TEXT, task TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE phase TEXT; resumed TEXT;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM relay_chat_jobs WHERE workspace=w AND reply=task AND state='waiting') THEN RETURN; END IF;
 IF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job=task AND state IN ('executing','uncertain')) THEN
  phase := 'attention';
 ELSIF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job=task AND requires_approval AND state='pending') THEN
  phase := 'approval';
 ELSIF EXISTS(SELECT 1 FROM relay_task_questions WHERE workspace=w AND reply=task AND state='pending') THEN
  phase := 'input';
 ELSE
  UPDATE relay_chat_jobs SET state='queued',claim=NULL,lease_until=0,attempts=0,
   run_cycle=run_cycle+1,finished_ms=NULL
  WHERE workspace=w AND reply=task AND state='waiting' AND run_cycle<20
   AND EXISTS(SELECT 1 FROM relay_chat_messages WHERE workspace=w AND id=task AND state='queued')
  RETURNING reply INTO resumed;
  IF resumed IS NULL THEN RETURN; END IF;
  phase := 'resuming';
 END IF;
 UPDATE relay_chat_messages SET progress=jsonb_build_object('state',phase,'text','')
 WHERE workspace=w AND id=task AND state='queued';
END $$;
CREATE FUNCTION relay_question_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM relay_resume_waiting_task(NEW.workspace,NEW.reply);
 -- Existing message revision/SSE transport delivers only authorized references.
 UPDATE relay_chat_messages SET content=content WHERE workspace=NEW.workspace AND id=NEW.reply;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_question_changed AFTER INSERT OR UPDATE OF state ON relay_task_questions
 FOR EACH ROW EXECUTE FUNCTION relay_question_changed();
CREATE FUNCTION relay_questions_stopped() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.state='finished' AND OLD.state<>'finished' THEN
  UPDATE relay_task_questions SET state='invalidated'
  WHERE workspace=NEW.workspace AND reply=NEW.reply AND state='pending';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_questions_stopped AFTER UPDATE OF state ON relay_chat_jobs
 FOR EACH ROW EXECUTE FUNCTION relay_questions_stopped();
