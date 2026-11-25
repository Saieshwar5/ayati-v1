-- Schema 151: one rule for everything an agent does on its own (the owner,
-- 2026-11-25, replacing the channel rule of schema 145). A task run happens in
-- the task's own conversation, as the person it belongs to, exactly like that
-- person asking the agent there. An event trigger runs in the conversation chosen
-- when it was set up, or in its person's direct chat with the agent. Work rooms,
-- a trigger's Memory setting, per-subject rooms and posted run results are gone.
-- While Cuckoo is in development no old data is kept for them.

-- The person's direct chat with the agent, found or opened like
-- shared_rooms::direct_room; NULL when the person or the agent is gone.
CREATE FUNCTION relay_migration_direct_chat(w TEXT, person TEXT, agent_id TEXT) RETURNS TEXT LANGUAGE plpgsql AS $$
DECLARE
 k TEXT := '['||to_json('agent:'||agent_id)::text||','||to_json('user:'||person)::text||']';
 found TEXT;
BEGIN
 SELECT id INTO found FROM relay_chats WHERE workspace=w AND direct_key=k;
 IF found IS NOT NULL THEN RETURN found; END IF;
 -- An assistant's chat with its person may predate direct keys.
 SELECT c.id INTO found FROM relay_chats c
  JOIN relay_chat_people p ON p.workspace=c.workspace AND p.chat=c.id AND p.user_id=person
  JOIN relay_chat_agents x ON x.workspace=c.workspace AND x.chat=c.id AND x.agent=agent_id
  WHERE c.workspace=w AND c.kind='direct' AND NOT c.work
   AND (SELECT count(*) FROM relay_chat_people q WHERE q.workspace=c.workspace AND q.chat=c.id)=1
   AND (SELECT count(*) FROM relay_chat_agents y WHERE y.workspace=c.workspace AND y.chat=c.id)=1
  ORDER BY c.id LIMIT 1;
 IF found IS NOT NULL THEN RETURN found; END IF;
 IF NOT EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=w AND m.user_id=person)
  OR NOT EXISTS(SELECT 1 FROM relay_shared_agents a WHERE a.workspace=w AND a.id=agent_id) THEN
  RETURN NULL;
 END IF;
 found := gen_random_uuid()::text;
 INSERT INTO relay_chats(id,workspace,name,creator,kind,description,direct_key)
  SELECT found,w,a.name,person,'direct','',k FROM relay_shared_agents a WHERE a.workspace=w AND a.id=agent_id;
 INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES(w,found,person);
 INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES(w,found,agent_id,0);
 RETURN found;
END $$;

-- Tasks. One whose conversation is real runs there; its work room is dropped. One
-- whose only place was a work room (no post target, Activity only, a weekly review)
-- moves to its person's direct chat with its agent. When that person has left the
-- workspace there is no chat to run in, so an open task is paused and says why.
DO $$
DECLARE t RECORD; place TEXT; now_ms BIGINT := (extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 FOR t IN SELECT x.workspace,x.id,x.requester,x.agent FROM relay_tasks x
   JOIN relay_chats c ON c.workspace=x.workspace AND c.id=x.chat WHERE c.work
   ORDER BY x.workspace,x.created_ms,x.id
 LOOP
  place := relay_migration_direct_chat(t.workspace,t.requester,t.agent);
  IF place IS NOT NULL THEN
   UPDATE relay_tasks SET chat=place WHERE workspace=t.workspace AND id=t.id;
  ELSE
   UPDATE relay_tasks SET paused_ms=COALESCE(paused_ms,now_ms),
     paused_reason='Paused because the person it was for is no longer in the workspace, so it has no conversation to run in.'
    WHERE workspace=t.workspace AND id=t.id AND outcome IS NULL;
   UPDATE relay_task_wakeups SET state='cancelled'
    WHERE workspace=t.workspace AND task=t.id AND state='pending';
  END IF;
 END LOOP;
END $$;
ALTER TABLE relay_tasks DROP COLUMN room;

-- Event triggers. The conversation a trigger posted to is where it runs now; one
-- that posted nowhere runs in its person's direct chat with the agent, and its task
-- with it. A trigger whose person has left keeps its task's old place and is
-- blocked (every trigger has a task since schema 148). The saved configuration is
-- otherwise unchanged, so its check is off meanwhile.
ALTER TABLE relay_event_triggers DISABLE TRIGGER relay_event_trigger_immutable;
UPDATE relay_event_triggers t SET chat=relay_migration_direct_chat(t.workspace,t.principal,t.agent)
 WHERE t.chat IS NULL;
UPDATE relay_event_triggers t SET chat=x.chat,state=CASE WHEN t.state IN ('enabled','paused') THEN 'blocked' ELSE t.state END,
  revision=t.revision+1,detail='The person who set it up is no longer in the workspace.'
 FROM relay_tasks x WHERE t.chat IS NULL AND x.workspace=t.workspace AND x.id=t.task;
UPDATE relay_event_triggers SET input=input-'memory' WHERE input ? 'memory';
UPDATE relay_tasks x SET chat=t.chat FROM relay_event_triggers t
 WHERE x.workspace=t.workspace AND x.id=t.task AND x.chat<>t.chat;
ALTER TABLE relay_event_triggers ENABLE TRIGGER relay_event_trigger_immutable;

CREATE OR REPLACE FUNCTION relay_event_trigger_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.workspace,NEW.id,NEW.agent,NEW.principal,NEW.source,NEW.binding,NEW.source_definition,
 NEW.connection,NEW.read_operation,NEW.chat,NEW.input,NEW.request_hash,NEW.created_ms)
 IS DISTINCT FROM ROW(OLD.workspace,OLD.id,OLD.agent,OLD.principal,OLD.source,OLD.binding,OLD.source_definition,
 OLD.connection,OLD.read_operation,OLD.chat,OLD.input,OLD.request_hash,OLD.created_ms) THEN
  RAISE EXCEPTION 'A saved event trigger configuration cannot change';
 END IF; RETURN NEW;
END $$;

-- A trigger's task belongs to the conversation the trigger runs in.
CREATE OR REPLACE FUNCTION relay_event_trigger_task(w TEXT, trigger_id TEXT) RETURNS TEXT LANGUAGE plpgsql AS $$
DECLARE
 x RECORD; tid TEXT; name TEXT; now_ms BIGINT := (extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 SELECT * INTO x FROM relay_event_triggers WHERE workspace=w AND id=trigger_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Event trigger unavailable'; END IF;
 IF x.task IS NOT NULL THEN RETURN x.task; END IF;
 name := COALESCE(NULLIF(btrim(x.input->>'name'),''),x.source_definition->>'name','Event');
 tid := gen_random_uuid()::text;
 INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,created_ms,instructions,
   paused_ms,paused_reason,outcome,result,closed_ms)
 VALUES(w,tid,x.agent,'commitment','event',left(name,160),x.principal,x.chat,x.created_ms,
   left(NULLIF(btrim(x.input->>'instructions'),''),10000),
   CASE WHEN x.state IN ('paused','blocked') THEN now_ms END,
   CASE x.state WHEN 'paused' THEN 'Its event trigger is paused.'
    WHEN 'blocked' THEN left(COALESCE(NULLIF(x.detail,''),'Its event trigger is blocked.'),300) END,
   CASE WHEN x.state='removed' THEN 'done' END,
   CASE WHEN x.state='removed' THEN 'Its event trigger was removed.' END,
   CASE WHEN x.state='removed' THEN now_ms END);
 UPDATE relay_event_triggers SET task=tid WHERE workspace=w AND id=trigger_id;
 RETURN tid;
END $$;

-- Memory, the remembering room, the room a delivery ran in and per-subject rooms
-- are gone. Deliveries keep their subject for tools marked "Needs the subject"; one
-- trigger's deliveries now run one at a time, in order, whatever their subject.
ALTER TABLE relay_event_triggers DROP COLUMN memory, DROP COLUMN room, ALTER COLUMN chat SET NOT NULL;
ALTER TABLE relay_event_deliveries DROP COLUMN room;
DROP INDEX relay_event_subject_pending;
DROP TABLE relay_subject_rooms;

-- Results are no longer copied anywhere: the run's reply is in its conversation.
DROP TABLE relay_run_posts;

-- Work rooms. Requests that never started there are cancelled; the rooms keep
-- their old messages but no people or agents, so nobody opens or uses them again.
UPDATE relay_chat_messages m SET state='cancelled',content='Cancelled: task runs now happen in the task''s conversation.'
 FROM relay_chat_jobs j, relay_chats c
 WHERE j.workspace=m.workspace AND j.reply=m.id AND j.state='queued' AND j.run_cycle=0 AND j.attempts=0
  AND c.workspace=j.workspace AND c.id=j.payload->>'chat' AND c.work AND m.state='queued';
UPDATE relay_chat_jobs j SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM relay_chats c
 WHERE j.state='queued' AND j.run_cycle=0 AND j.attempts=0 AND c.workspace=j.workspace AND c.id=j.payload->>'chat' AND c.work;
DELETE FROM relay_chat_people p USING relay_chats c WHERE c.workspace=p.workspace AND c.id=p.chat AND c.work;
DELETE FROM relay_chat_agents x USING relay_chats c WHERE c.workspace=x.workspace AND c.id=x.chat AND c.work;
ALTER TABLE relay_chats DROP CONSTRAINT relay_chats_work_private;
ALTER TABLE relay_chats DROP COLUMN work;
DROP FUNCTION relay_migration_direct_chat(TEXT, TEXT, TEXT);

-- A run happens in its task's conversation, so a "viewers" Library item may be used
-- only if everyone in that conversation may see it: the reply's own conversation.
CREATE OR REPLACE FUNCTION relay_library_agent_use(TEXT, TEXT, TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT g.use_for
 FROM relay_chat_jobs j
 JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
 JOIN relay_library_items i ON i.workspace=j.workspace AND i.id=$2 AND NOT i.trashed
 JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id) AND NOT s.trashed
 JOIN relay_library_agents g ON g.workspace=s.workspace AND g.item=s.id AND g.agent=$3
 WHERE j.workspace=$1 AND j.reply=$4
  AND NOT EXISTS(SELECT 1 FROM relay_communication_inbox x
   WHERE x.workspace=$1 AND x.reply=$4 AND x.transport<>'chat')
  AND (g.use_for='staff' OR (g.use_for='viewers'
   AND relay_library_access($1,$2,j.payload->>'requester') IS NOT NULL
   AND NOT EXISTS(
    SELECT 1 FROM relay_members p
    WHERE p.workspace=$1 AND relay_library_access($1,$2,p.user_id) IS NULL
     AND EXISTS(SELECT 1 FROM relay_chats c
      WHERE c.workspace=$1 AND c.id=m.chat
       AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people cp
        WHERE cp.workspace=$1 AND cp.chat=c.id AND cp.user_id=p.user_id))))))
$$;
