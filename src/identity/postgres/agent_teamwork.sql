-- Schema 124: people and agents working together in channels.
--
-- What a person handles, one line on their workspace profile ("Prices,
-- discounts"). Agents read it to know whom to ask; it is a description,
-- never a permission. An agent's own line lives in its settings.
ALTER TABLE relay_member_profiles ADD COLUMN handles TEXT NOT NULL DEFAULT ''
 CHECK(char_length(handles)<=160);

-- An agent asks a person in the conversation one question and waits, the way
-- it waits for an approval or a helper agent. The person's next message in the
-- same place is the answer: information for the agent, never an approval.
CREATE TABLE relay_person_asks (
 workspace TEXT NOT NULL, id TEXT NOT NULL, reply TEXT NOT NULL,
 request_key TEXT NOT NULL CHECK(length(request_key) BETWEEN 1 AND 64),
 run_cycle INTEGER NOT NULL,
 chat TEXT NOT NULL, thread TEXT,
 person TEXT NOT NULL, question TEXT NOT NULL CHECK(char_length(question) BETWEEN 1 AND 500),
 -- The agent's message that asked, which @mentions the person.
 message TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','answered','expired','cancelled')),
 answer TEXT CHECK(char_length(answer)<=4000), answer_message TEXT, answered_ms BIGINT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 deadline_ms BIGINT NOT NULL,
 PRIMARY KEY(workspace,id), UNIQUE(workspace,reply,request_key),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply),
 CHECK((state='answered') = (answer_message IS NOT NULL))
);
CREATE UNIQUE INDEX relay_one_pending_person_ask ON relay_person_asks(workspace,reply) WHERE state='pending';
CREATE INDEX relay_person_asks_waiting ON relay_person_asks(workspace,chat,person) WHERE state='pending';

-- A task waiting on a person stays waiting; answered or expired, it resumes.
CREATE OR REPLACE FUNCTION relay_resume_waiting_task(w TEXT,task TEXT) RETURNS void LANGUAGE plpgsql AS $$
DECLARE phase TEXT;resumed TEXT;
BEGIN
 IF NOT EXISTS(SELECT 1 FROM relay_chat_jobs WHERE workspace=w AND reply= $2 AND state='waiting') THEN RETURN;END IF;
 IF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job= $2 AND state IN ('executing','uncertain')) THEN phase:='attention';
 ELSIF EXISTS(SELECT 1 FROM relay_control_actions WHERE workspace=w AND job= $2 AND requires_approval AND state='pending') THEN phase:='approval';
 ELSIF EXISTS(SELECT 1 FROM relay_task_questions WHERE workspace=w AND reply= $2 AND state='pending') THEN phase:='input';
 ELSIF EXISTS(SELECT 1 FROM relay_agent_handoffs WHERE workspace=w AND parent= $2 AND state='pending') THEN phase:='handoff';
 ELSIF EXISTS(SELECT 1 FROM relay_person_asks WHERE workspace=w AND reply= $2 AND state='pending') THEN phase:='person';
 ELSE
  UPDATE relay_chat_jobs SET state='queued',claim=NULL,lease_until=0,attempts=0,run_cycle=run_cycle+1,finished_ms=NULL
  WHERE workspace=w AND reply= $2 AND state='waiting' AND run_cycle<20
  AND EXISTS(SELECT 1 FROM relay_chat_messages WHERE workspace=w AND id= $2 AND state='queued') RETURNING reply INTO resumed;
  IF resumed IS NULL THEN RETURN;END IF;phase:='resuming';
 END IF;
 UPDATE relay_chat_messages SET progress=jsonb_build_object('state',phase,'text','') WHERE workspace=w AND id= $2 AND state='queued';
END $$;

CREATE FUNCTION relay_person_ask_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 PERFORM relay_resume_waiting_task(NEW.workspace,NEW.reply);
 RETURN NEW;
END $$;
CREATE TRIGGER relay_person_ask_changed AFTER INSERT OR UPDATE OF state ON relay_person_asks
 FOR EACH ROW EXECUTE FUNCTION relay_person_ask_changed();

-- The asked person's next message in the same place answers.
CREATE FUNCTION relay_person_ask_answered() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.kind='user' AND NEW.state='sent' THEN
  UPDATE relay_person_asks a SET state='answered',answer=left(NEW.content,4000),answer_message=NEW.id,
   answered_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
  WHERE a.workspace=NEW.workspace AND a.chat=NEW.chat AND a.person=NEW.author AND a.state='pending'
   AND a.thread IS NOT DISTINCT FROM NEW.thread;
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_person_ask_answered AFTER INSERT ON relay_chat_messages
 FOR EACH ROW EXECUTE FUNCTION relay_person_ask_answered();

-- A finished or stopped task stops waiting for anyone.
CREATE FUNCTION relay_person_asks_stopped() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.state='finished' AND OLD.state<>'finished' THEN
  UPDATE relay_person_asks SET state='cancelled' WHERE workspace=NEW.workspace AND reply=NEW.reply AND state='pending';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_person_asks_stopped AFTER UPDATE OF state ON relay_chat_jobs
 FOR EACH ROW EXECUTE FUNCTION relay_person_asks_stopped();
