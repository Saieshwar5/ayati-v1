CREATE TABLE relay_agent_handoffs (
 workspace TEXT NOT NULL,id TEXT NOT NULL,parent TEXT NOT NULL,root TEXT NOT NULL,
 child TEXT NOT NULL,request_message TEXT NOT NULL,request_key TEXT NOT NULL,input_hash TEXT NOT NULL,
 principal TEXT NOT NULL,sender TEXT NOT NULL,target TEXT NOT NULL,chat TEXT NOT NULL,thread TEXT NOT NULL,
 task TEXT NOT NULL CHECK(octet_length(task)<=4000),controls JSONB NOT NULL CHECK(octet_length(controls::text)<=65536),
 source JSONB,source_revoked BOOLEAN NOT NULL DEFAULT FALSE,
 depth INTEGER NOT NULL CHECK(depth BETWEEN 1 AND 3),run_cycle INTEGER NOT NULL,
 expires_ms BIGINT NOT NULL,checked_ms BIGINT NOT NULL DEFAULT 0,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','complete','failed','cancelled','expired')),
 detail TEXT NOT NULL DEFAULT '',created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),UNIQUE(workspace,parent,request_key),UNIQUE(workspace,parent,input_hash),UNIQUE(workspace,child),UNIQUE(request_message),
 FOREIGN KEY(workspace,parent) REFERENCES relay_chat_jobs(workspace,reply),
 FOREIGN KEY(workspace,root) REFERENCES relay_chat_jobs(workspace,reply),
 FOREIGN KEY(workspace,child) REFERENCES relay_chat_jobs(workspace,reply),
 FOREIGN KEY(workspace,request_message) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(principal) REFERENCES relay_users(id),
 FOREIGN KEY(workspace,sender) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,target) REFERENCES relay_shared_agents(workspace,id),
 CHECK(sender<>target AND parent<>child AND root<>child)
);
CREATE INDEX relay_handoff_root ON relay_agent_handoffs(workspace,root);
CREATE INDEX relay_handoff_wait ON relay_agent_handoffs(expires_ms,workspace,parent) WHERE state='pending';
CREATE FUNCTION relay_handoff_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF (to_jsonb(NEW)-'state'-'detail'-'source_revoked'-'checked_ms') IS DISTINCT FROM (to_jsonb(OLD)-'state'-'detail'-'source_revoked'-'checked_ms')
 OR (OLD.source_revoked AND NOT NEW.source_revoked)
 OR (OLD.state<>'pending' AND NEW.state<>OLD.state) THEN
  RAISE EXCEPTION 'Saved handoff provenance and terminal outcomes cannot change';
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER relay_handoff_immutable BEFORE UPDATE ON relay_agent_handoffs FOR EACH ROW EXECUTE FUNCTION relay_handoff_immutable();

-- Exact cancellation is shared by explicit parent Stop, expiration and revocation.
-- No executed effect is rewritten or reported as undone.
CREATE FUNCTION relay_handoff_stop_tasks(w TEXT,tasks TEXT[],reason TEXT) RETURNS void LANGUAGE plpgsql AS $$
BEGIN
 UPDATE relay_chat_jobs SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE workspace=w AND reply=ANY(tasks) AND state<>'finished';
 UPDATE relay_control_actions SET state='invalidated',result=to_jsonb(reason)
 WHERE workspace=w AND job=ANY(tasks) AND state='pending';
 UPDATE relay_worker_assignments SET state='finished',outcome='cancelled',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE workspace=w AND reply=ANY(tasks) AND state IN ('launching','running');
 UPDATE relay_assignment_credentials c SET context=NULL,expires=0 WHERE EXISTS(SELECT 1 FROM relay_worker_assignments a WHERE a.id=c.assignment AND a.workspace=w AND a.reply=ANY(tasks));
 UPDATE relay_chat_messages SET state='cancelled',content=reason,progress=NULL
 WHERE workspace=w AND id=ANY(tasks) AND state='queued';
END $$;
CREATE FUNCTION relay_handoff_cancel_branch(w TEXT,task TEXT,reason TEXT,outcome TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE tasks text[];
BEGIN
 WITH RECURSIVE branch(child) AS (
  SELECT child FROM relay_agent_handoffs WHERE workspace=w AND (parent= $2 OR child= $2)
  UNION SELECT h.child FROM relay_agent_handoffs h JOIN branch b ON h.parent=b.child WHERE h.workspace=w
 ) SELECT array_agg(child) INTO tasks FROM branch;
 IF tasks IS NULL THEN RETURN;END IF;
 UPDATE relay_agent_handoffs SET state=outcome,detail=reason WHERE workspace=w AND child=ANY(tasks) AND state='pending';
 PERFORM relay_handoff_stop_tasks(w,tasks,reason);
END $$;
CREATE OR REPLACE FUNCTION relay_resume_waiting_task(w TEXT,task TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE phase TEXT;resumed TEXT;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM relay_chat_jobs WHERE workspace=w AND reply= $2 AND state='waiting') THEN RETURN;END IF;
 IF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job= $2 AND state IN ('executing','uncertain')) THEN phase:='attention';
 ELSIF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job= $2 AND requires_approval AND state='pending') THEN phase:='approval';
 ELSIF EXISTS(SELECT 1 FROM relay_task_questions WHERE workspace=w AND reply= $2 AND state='pending') THEN phase:='input';
 ELSIF EXISTS(SELECT 1 FROM relay_agent_handoffs WHERE workspace=w AND parent= $2 AND state='pending') THEN phase:='handoff';
 ELSE
  UPDATE relay_chat_jobs SET state='queued',claim=NULL,lease_until=0,attempts=0,run_cycle=run_cycle+1,finished_ms=NULL
  WHERE workspace=w AND reply= $2 AND state='waiting' AND run_cycle<20
  AND EXISTS(SELECT 1 FROM relay_chat_messages WHERE workspace=w AND id= $2 AND state='queued') RETURNING reply INTO resumed;
  IF resumed IS NULL THEN RETURN;END IF;phase:='resuming';
 END IF;
 UPDATE relay_chat_messages SET progress=jsonb_build_object('state',phase,'text','') WHERE workspace=w AND id= $2 AND state='queued';
END $$;
CREATE FUNCTION relay_handoff_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM relay_resume_waiting_task(NEW.workspace,NEW.parent);
 UPDATE relay_chat_messages SET content=content WHERE workspace=NEW.workspace AND id IN (NEW.parent,NEW.child,NEW.request_message);
 RETURN NEW;
END $$;
CREATE TRIGGER relay_handoff_changed AFTER INSERT OR UPDATE OF state,source_revoked ON relay_agent_handoffs FOR EACH ROW EXECUTE FUNCTION relay_handoff_changed();
CREATE FUNCTION relay_handoff_task_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.state IS NOT DISTINCT FROM OLD.state OR NEW.state NOT IN ('complete','failed','cancelled') THEN RETURN NEW;END IF;
 UPDATE relay_agent_handoffs SET state=NEW.state,detail=CASE WHEN NEW.state='complete' THEN '' ELSE 'Helper task ended without a completed result.' END
 WHERE workspace=NEW.workspace AND child=NEW.id AND state='pending';
 IF NEW.state<>'complete' THEN
  PERFORM relay_handoff_cancel_branch(NEW.workspace,NEW.id,'Original task ended; unfinished helper work was stopped.','cancelled');
 END IF;RETURN NEW;
END $$;
CREATE TRIGGER relay_handoff_task_changed AFTER UPDATE OF state ON relay_chat_messages FOR EACH ROW EXECUTE FUNCTION relay_handoff_task_changed();
