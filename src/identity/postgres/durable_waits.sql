-- Recovery attempts are bounded per continuation; assignment generations never repeat.
ALTER TABLE relay_chat_jobs DROP CONSTRAINT relay_chat_jobs_state_check;
ALTER TABLE relay_chat_jobs ADD CHECK(state IN ('queued','running','waiting','finished'));
ALTER TABLE relay_chat_jobs ADD COLUMN generation INTEGER NOT NULL DEFAULT 0,
 ADD COLUMN run_cycle INTEGER NOT NULL DEFAULT 0 CHECK(run_cycle BETWEEN 0 AND 20);
UPDATE relay_chat_jobs SET generation=attempts;
ALTER TABLE relay_worker_assignments DROP CONSTRAINT relay_worker_assignments_generation_check;
ALTER TABLE relay_worker_assignments ADD CHECK(generation>0);
ALTER TABLE relay_control_actions ADD COLUMN run_cycle INTEGER,
 ADD COLUMN requires_approval BOOLEAN NOT NULL DEFAULT FALSE;
CREATE TABLE relay_task_checkpoints (
 workspace TEXT NOT NULL, reply TEXT NOT NULL, cycle INTEGER NOT NULL,
 claim TEXT NOT NULL, text TEXT NOT NULL CHECK(octet_length(text)<=65536),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,reply,cycle), UNIQUE(claim),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply)
);

-- Called in the same transaction as checkpointing or a saved action transition.
-- A decision before release and a decision after release have the same outcome.
CREATE FUNCTION relay_resume_waiting_task(w TEXT, task TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE phase TEXT; resumed TEXT;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM relay_chat_jobs WHERE workspace=w AND reply=task AND state='waiting') THEN RETURN; END IF;
 IF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job=task AND state IN ('executing','uncertain')) THEN
  phase := 'attention';
 ELSIF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job=task AND requires_approval AND state='pending') THEN
  phase := 'approval';
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
CREATE FUNCTION relay_task_action_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM relay_resume_waiting_task(NEW.workspace,NEW.job);
 RETURN NEW;
END $$;
CREATE TRIGGER relay_task_action_changed AFTER UPDATE ON relay_control_actions
 FOR EACH ROW EXECUTE FUNCTION relay_task_action_changed();
