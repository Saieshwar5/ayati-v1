-- Schema 150: the old schedule tables and everything that only served them are
-- dropped (designs/agent-tasks/one-system.md). Schema 147 made every schedule a
-- task and linked its earlier runs to that task; schema 146 moved Home panel
-- intervals to the panel's own update queue. Nothing reads a schedule any more.

-- Tasks keep no schedule origin: a converted schedule was set up by a person, and
-- an automatic task a schedule run opened tracked an ordinary request.
UPDATE relay_tasks SET origin=CASE WHEN kind='commitment' THEN 'person' ELSE 'request' END
 WHERE origin='schedule';
ALTER TABLE relay_tasks DROP CONSTRAINT relay_tasks_origin_check;
ALTER TABLE relay_tasks ADD CONSTRAINT relay_tasks_origin_check
 CHECK(origin IN ('request','event','agent','person','check'));
ALTER TABLE relay_tasks DROP COLUMN schedule;

-- A weekly review is its task; a routine without one never ran after schema 147.
DELETE FROM relay_review_routines WHERE task IS NULL;
ALTER TABLE relay_review_routines DROP COLUMN schedule, ALTER COLUMN task SET NOT NULL;

-- A panel still linked to a paused or blocked schedule (schema 146 left those for
-- review) has no next update; its update agreement pauses so its owner reviews it.
UPDATE relay_home_maintenance SET enabled=FALSE WHERE schedule IS NOT NULL AND enabled;
ALTER TABLE relay_home_maintenance DROP COLUMN schedule;
DROP TABLE relay_home_schedule_bindings;

-- An agent may use a Library item in a reply only if everyone who will see it may:
-- the reply's conversation and, for a task's run, the conversation its result is
-- posted to (the task's chat). This replaces the schedule's posting conversation.
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
      WHERE c.workspace=$1
       AND (c.id=m.chat OR c.id IN (SELECT t.chat FROM relay_tasks t WHERE t.workspace=$1 AND t.id=j.task))
       AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people cp
        WHERE cp.workspace=$1 AND cp.chat=c.id AND cp.user_id=p.user_id))))))
$$;

-- The schedules themselves: their runs, changes, proposals and safety triggers.
DROP TRIGGER relay_schedule_principal_changed ON relay_members;
DROP FUNCTION relay_schedule_principal_changed();
DROP TABLE relay_schedule_occurrences, relay_schedule_changes, relay_schedule_drafts;
DROP TABLE relay_agent_schedules;
DROP FUNCTION relay_schedule_immutable();
