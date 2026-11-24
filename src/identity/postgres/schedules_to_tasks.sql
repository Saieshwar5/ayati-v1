-- Schema 147: schedules become tasks (designs/agent-tasks/one-system.md, decision 4).
-- Every schedule that is not a Home panel update (schema 146 moved those to the
-- panel's own queue) becomes a commitment task for its person: title = name,
-- instructions = content, the schedule's time zone, its first run as anchor and a
-- calendar rule from its interval. It works in its own work room and posts where it
-- posted before (decision 1); one with no post target belongs to its work room and
-- shows only in Activity. Its earlier runs are linked to the task, and the schedule
-- row is marked moved (state completed) so nothing runs it again.
ALTER TABLE relay_agent_schedules
 ADD COLUMN task TEXT,
 ADD CONSTRAINT relay_agent_schedules_task FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id);
-- An agent's weekly review is a task from now on (it was a schedule).
ALTER TABLE relay_review_routines
 ALTER COLUMN schedule DROP NOT NULL,
 ADD COLUMN task TEXT,
 ADD CONSTRAINT relay_review_routines_task FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id);

DO $$
DECLARE
 s RECORD;
 tid TEXT; work_room TEXT; zone TEXT; local_first TIMESTAMP; rule JSONB; hours INT; done INT;
 automatic TEXT[];
BEGIN
 FOR s IN SELECT x.* FROM relay_agent_schedules x
   WHERE x.task IS NULL
     AND NOT EXISTS(SELECT 1 FROM relay_home_schedule_bindings b WHERE b.workspace=x.workspace AND b.schedule=x.id)
   ORDER BY x.workspace,x.created_ms,x.id
 LOOP
  local_first := to_timestamp(s.first_ms/1000.0) AT TIME ZONE s.timezone;
  -- Whole weeks run weekly on the first run's weekday and time, whole days daily,
  -- whole hours that divide a day hourly at the same minute; anything else keeps
  -- its exact period as a minute rule. A one-time schedule has no rule.
  IF s.interval_seconds IS NULL THEN
   rule := NULL;
  ELSIF s.interval_seconds % 604800 = 0 THEN
   rule := jsonb_build_object('unit','week','every',s.interval_seconds/604800,
    'days',jsonb_build_array(extract(isodow FROM local_first)::int),'at',to_char(local_first,'HH24:MI'));
  ELSIF s.interval_seconds % 86400 = 0 THEN
   rule := jsonb_build_object('unit','day','every',s.interval_seconds/86400,'at',to_char(local_first,'HH24:MI'));
  ELSIF s.interval_seconds % 3600 = 0 AND 24 % (s.interval_seconds/3600) = 0 THEN
   hours := s.interval_seconds/3600;
   rule := jsonb_build_object('unit','hour','every',hours,
    'from',lpad((extract(hour FROM local_first)::int % hours)::text,2,'0')||':'||to_char(local_first,'MI'));
  ELSE
   rule := jsonb_build_object('unit','minute','every',GREATEST(s.interval_seconds/60,1));
  END IF;
  zone := CASE WHEN length(s.timezone)<=64 THEN s.timezone END;

  -- Its own work room, opened now if it never ran or started fresh each run.
  work_room := s.room;
  IF work_room IS NULL THEN
   work_room := gen_random_uuid()::text;
   INSERT INTO relay_chats(id,workspace,name,creator,kind,description,work)
    VALUES(work_room,s.workspace,s.name,s.principal,'direct','Runs of a task',TRUE);
   -- A person who has left the workspace can't be in it; the task's runs then block.
   INSERT INTO relay_chat_people(workspace,chat,user_id)
    SELECT s.workspace,work_room,s.principal
    WHERE EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=s.workspace AND m.user_id=s.principal);
   INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES(s.workspace,work_room,s.agent,0);
  END IF;

  done := (SELECT count(*) FROM relay_schedule_occurrences o WHERE o.workspace=s.workspace AND o.schedule=s.id);
  tid := gen_random_uuid()::text;
  INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,room,created_ms,
    instructions,repeat,timezone,anchor_ms,schedule,runs,paused_ms,paused_reason,outcome,closed_ms)
  VALUES(s.workspace,tid,s.agent,'commitment','schedule',left(s.name,160),s.principal,
    COALESCE(s.chat,work_room),CASE WHEN s.chat IS NOT NULL THEN work_room END,s.created_ms,
    left(s.content,10000),rule,zone,s.first_ms,s.id,done,
    CASE WHEN s.state IN ('paused','blocked') THEN (extract(epoch FROM clock_timestamp())*1000)::bigint END,
    CASE s.state WHEN 'paused' THEN 'Paused before it became a task.'
     WHEN 'blocked' THEN left(COALESCE(NULLIF(s.detail,''),'Blocked before it became a task.'),300) END,
    CASE WHEN s.state='completed' THEN 'done' END,
    CASE WHEN s.state='completed' THEN COALESCE(
     (SELECT max(o.created_ms) FROM relay_schedule_occurrences o WHERE o.workspace=s.workspace AND o.schedule=s.id),
     s.created_ms) END);

  -- An enabled schedule keeps its next run, now booked on the task.
  IF s.state='enabled' AND s.next_ms IS NOT NULL THEN
   INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind)
    VALUES(s.workspace,gen_random_uuid()::text,tid,s.next_ms,CASE WHEN rule IS NULL THEN 'follow_up' ELSE 'repeat' END);
  END IF;

  -- Its earlier runs belong to the task; an automatic task a run had is dropped
  -- when nothing else works towards it (as attach_to_task does).
  automatic := ARRAY(SELECT j.task FROM relay_schedule_occurrences o
   JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.reply
   JOIN relay_tasks t ON t.workspace=j.workspace AND t.id=j.task
   WHERE o.workspace=s.workspace AND o.schedule=s.id AND t.kind='automatic');
  UPDATE relay_chat_jobs j SET task=tid
   FROM relay_schedule_occurrences o
   WHERE o.workspace=s.workspace AND o.schedule=s.id AND j.workspace=o.workspace AND j.reply=o.reply
     AND (j.task IS NULL OR j.task=ANY(automatic));
  DELETE FROM relay_tasks t WHERE t.workspace=s.workspace AND t.id=ANY(automatic) AND t.kind='automatic'
   AND NOT EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.task=t.id);

  -- The schedule no longer runs. Its configuration stays as it was saved.
  UPDATE relay_agent_schedules SET task=tid,state='completed',next_ms=NULL,revision=revision+1,detail='Moved to a task.'
   WHERE workspace=s.workspace AND id=s.id;
 END LOOP;
END $$;

UPDATE relay_review_routines r SET task=x.task
 FROM relay_agent_schedules x WHERE x.workspace=r.workspace AND x.id=r.schedule AND x.task IS NOT NULL;
