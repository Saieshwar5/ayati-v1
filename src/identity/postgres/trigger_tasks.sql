-- Schema 148: event triggers become task wake-ups (designs/agent-tasks/one-system.md,
-- decision 6). Each trigger gets a commitment task for its person: title = the
-- trigger's name, instructions = its task text. A delivered event starts a run of
-- that task (a wake-up of kind 'event'), so its runs are counted and shown like any
-- task's. The trigger keeps its reviewed source, fields, filter, deliveries and
-- memory. Its state and its task's stay together: pausing, resuming or ending one
-- does the same to the other.
ALTER TABLE relay_event_triggers
 ADD COLUMN task TEXT,
 ADD CONSTRAINT relay_event_triggers_task FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id);
CREATE UNIQUE INDEX relay_event_triggers_task_once ON relay_event_triggers(workspace,task) WHERE task IS NOT NULL;
ALTER TABLE relay_task_wakeups DROP CONSTRAINT relay_task_wakeups_kind_check;
ALTER TABLE relay_task_wakeups ADD CONSTRAINT relay_task_wakeups_kind_check
 CHECK(kind IN ('follow_up','repeat','end','continue','event'));

-- The task of trigger `trigger_id`, made the first time it is asked for. It belongs
-- where the trigger posts, with its work room as `room`; a trigger that posts
-- nowhere belongs to its work room and shows only in Activity. The work room is the
-- trigger's remembering room, opened now if there is none; a trigger that starts
-- fresh keeps opening a room per run, as it did.
CREATE FUNCTION relay_event_trigger_task(w TEXT, trigger_id TEXT) RETURNS TEXT LANGUAGE plpgsql AS $$
DECLARE
 x RECORD; tid TEXT; work_room TEXT; name TEXT; now_ms BIGINT := (extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 SELECT * INTO x FROM relay_event_triggers WHERE workspace=w AND id=trigger_id FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'Event trigger unavailable'; END IF;
 IF x.task IS NOT NULL THEN RETURN x.task; END IF;
 name := COALESCE(NULLIF(btrim(x.input->>'name'),''),x.source_definition->>'name','Event');
 work_room := x.room;
 IF work_room IS NULL THEN
  work_room := gen_random_uuid()::text;
  INSERT INTO relay_chats(id,workspace,name,creator,kind,description,work)
   VALUES(work_room,w,name,x.principal,'direct','Runs of an event trigger',TRUE);
  -- A person who has left the workspace can't be in it.
  INSERT INTO relay_chat_people(workspace,chat,user_id)
   SELECT w,work_room,x.principal WHERE EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=w AND m.user_id=x.principal);
  INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES(w,work_room,x.agent,0);
  IF x.memory='remember' THEN
   UPDATE relay_event_triggers SET room=work_room WHERE workspace=w AND id=trigger_id;
  END IF;
 END IF;
 tid := gen_random_uuid()::text;
 INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,room,created_ms,instructions,
   paused_ms,paused_reason,outcome,result,closed_ms)
 VALUES(w,tid,x.agent,'commitment','event',left(name,160),x.principal,COALESCE(x.chat,work_room),
   CASE WHEN x.chat IS NOT NULL THEN work_room END,x.created_ms,left(NULLIF(btrim(x.input->>'instructions'),''),10000),
   CASE WHEN x.state IN ('paused','blocked') THEN now_ms END,
   CASE x.state WHEN 'paused' THEN 'Its event trigger is paused.'
    WHEN 'blocked' THEN left(COALESCE(NULLIF(x.detail,''),'Its event trigger is blocked.'),300) END,
   CASE WHEN x.state='removed' THEN 'done' END,
   CASE WHEN x.state='removed' THEN 'Its event trigger was removed.' END,
   CASE WHEN x.state='removed' THEN now_ms END);
 UPDATE relay_event_triggers SET task=tid WHERE workspace=w AND id=trigger_id;
 RETURN tid;
END $$;

-- A trigger's state reaches its task, whatever changed it (its own controls, an
-- access change, a closed workspace): paused or blocked pauses the task with the
-- reason, enabled resumes it, removed ends it. The task's own Pause and End change
-- the trigger in code first, so here they find the task already changed.
CREATE FUNCTION relay_event_trigger_task_state() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE now_ms BIGINT := (extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 IF NEW.state IN ('paused','blocked') THEN
  UPDATE relay_tasks SET paused_ms=COALESCE(paused_ms,now_ms),
    paused_reason=CASE WHEN NEW.state='blocked' THEN left(COALESCE(NULLIF(NEW.detail,''),'Its event trigger was blocked.'),300)
     ELSE 'Its event trigger was paused.' END
   WHERE workspace=NEW.workspace AND id=NEW.task AND outcome IS NULL AND (paused_ms IS NULL OR NEW.state='blocked');
  UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=NEW.workspace AND task=NEW.task AND state='pending';
 ELSIF NEW.state='enabled' THEN
  UPDATE relay_tasks SET paused_ms=NULL,paused_reason=NULL WHERE workspace=NEW.workspace AND id=NEW.task AND outcome IS NULL;
 ELSIF NEW.state='removed' THEN
  UPDATE relay_tasks SET outcome='done',result='Its event trigger was removed.',closed_ms=now_ms
   WHERE workspace=NEW.workspace AND id=NEW.task AND outcome IS NULL;
  UPDATE relay_task_wakeups SET state='cancelled' WHERE workspace=NEW.workspace AND task=NEW.task AND state='pending';
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER relay_event_trigger_task_state AFTER UPDATE OF state ON relay_event_triggers
 FOR EACH ROW WHEN (NEW.state IS DISTINCT FROM OLD.state AND NEW.task IS NOT NULL)
 EXECUTE FUNCTION relay_event_trigger_task_state();

-- Every existing trigger gets its task. Its earlier runs belong to the task: each
-- delivery that started work is a run of it, and an automatic task a run had is
-- dropped when nothing else works towards it (as attach_to_task does).
DO $$
DECLARE
 x RECORD; tid TEXT; automatic TEXT[];
BEGIN
 FOR x IN SELECT workspace,id FROM relay_event_triggers WHERE task IS NULL ORDER BY workspace,created_ms,id LOOP
  tid := relay_event_trigger_task(x.workspace,x.id);
  automatic := ARRAY(SELECT j.task FROM relay_event_deliveries d
   JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply
   JOIN relay_tasks t ON t.workspace=j.workspace AND t.id=j.task
   WHERE d.workspace=x.workspace AND d.trigger_id=x.id AND t.kind='automatic');
  UPDATE relay_chat_jobs j SET task=tid
   FROM relay_event_deliveries d
   WHERE d.workspace=x.workspace AND d.trigger_id=x.id AND j.workspace=d.workspace AND j.reply=d.reply
     AND (j.task IS NULL OR j.task=ANY(automatic));
  DELETE FROM relay_tasks t WHERE t.workspace=x.workspace AND t.id=ANY(automatic) AND t.kind='automatic'
   AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.task=t.id);
  INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind,state,reply,message,run,created_ms)
   SELECT d.workspace,gen_random_uuid()::text,tid,i.received_ms,'event','started',d.reply,d.message,
    row_number() OVER (ORDER BY i.sequence,d.id),i.received_ms
   FROM relay_event_deliveries d JOIN relay_event_inbox i ON i.workspace=d.workspace AND i.source=d.source AND i.id=d.event_id
   WHERE d.workspace=x.workspace AND d.trigger_id=x.id AND d.state='accepted';
  UPDATE relay_tasks SET runs=(SELECT count(*) FROM relay_task_wakeups k WHERE k.workspace=x.workspace AND k.task=tid AND k.kind='event')
   WHERE workspace=x.workspace AND id=tid;
 END LOOP;
END $$;
