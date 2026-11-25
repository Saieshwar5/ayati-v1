-- Schema 149: an assistant's regular check becomes a task (designs/agent-tasks/
-- one-system.md, decision 7): "Regular check" for its person, on a minute rule with
-- the chosen interval, with the code check before the model (precheck 'messages':
-- direct messages and mentions for the person since the last check) and their
-- checklist as instructions. It runs where checks ran: the "Regular check" thread in
-- the person's chat with their assistant. The Regular checks panel edits the task;
-- relay_assistant_settings keeps the panel's choices and `checked`, the newest
-- message the last check looked at. Checks that are off have no task until they
-- are turned on.
ALTER TABLE relay_assistant_settings
 ADD COLUMN task TEXT,
 ADD CONSTRAINT relay_assistant_settings_task FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id);
ALTER TABLE relay_tasks DROP CONSTRAINT relay_tasks_origin_check;
ALTER TABLE relay_tasks ADD CONSTRAINT relay_tasks_origin_check
 CHECK(origin IN ('request','schedule','event','agent','person','check'));

DO $$
DECLARE
 c RECORD; tid TEXT; place TEXT; next_run BIGINT;
 now_ms BIGINT := (extract(epoch FROM clock_timestamp())*1000)::bigint;
BEGIN
 FOR c IN SELECT x.*,s.agent FROM relay_assistant_settings x
   JOIN relay_assistants s ON s.workspace=x.workspace AND s.user_id=x.user_id
   WHERE x.checks AND x.task IS NULL ORDER BY x.workspace,x.user_id
 LOOP
  -- The person's chat with their assistant, where the check thread is.
  place := COALESCE(
   (SELECT t.chat FROM relay_assistant_threads t WHERE t.workspace=c.workspace AND t.agent=c.agent AND t.automation='checks'),
   (SELECT ch.id FROM relay_chats ch
     JOIN relay_chat_agents a ON a.workspace=ch.workspace AND a.chat=ch.id AND a.agent=c.agent
     JOIN relay_chat_people p ON p.workspace=ch.workspace AND p.chat=ch.id AND p.user_id=c.user_id
     WHERE ch.workspace=c.workspace AND ch.kind='direct' AND NOT ch.work ORDER BY ch.id LIMIT 1));
  -- Without that chat no check could run before either; turning checks on again
  -- makes the task once the chat exists.
  CONTINUE WHEN place IS NULL;
  next_run := COALESCE(c.next_ms, now_ms + c.every_minutes::bigint*60000);
  tid := gen_random_uuid()::text;
  INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,created_by,instructions,repeat,anchor_ms,precheck)
  VALUES(c.workspace,tid,c.agent,'commitment','check','Regular check',c.user_id,place,c.user_id,
    NULLIF(btrim(c.checklist),''),jsonb_build_object('unit','minute','every',c.every_minutes),
    next_run - c.every_minutes::bigint*60000,'messages');
  INSERT INTO relay_task_wakeups(workspace,id,task,due_ms,kind)
   VALUES(c.workspace,gen_random_uuid()::text,tid,next_run,'repeat');
  -- A check still running belongs to the task, so the next one waits for it.
  UPDATE relay_chat_jobs SET task=tid WHERE workspace=c.workspace AND reply=c.job AND task IS NULL;
  UPDATE relay_assistant_settings SET task=tid,next_ms=NULL WHERE workspace=c.workspace AND user_id=c.user_id;
 END LOOP;
 -- Nothing reads the old due time any more.
 UPDATE relay_assistant_settings SET next_ms=NULL WHERE next_ms IS NOT NULL;
END $$;
