-- Schema 145: one system for everything agents do on their own
-- (designs/agent-tasks/one-system.md), first step.
--   instructions: what each run must do (a converted schedule keeps its content);
--   room: the private work room where a channel's task works, posting each run's
--     result to the channel (the owner's channel rule); null means it runs in its
--     own conversation;
--   precheck: a check in code before the model ('messages': anything new for the
--     person since the last run), for regular checks;
--   schedule: the schedule a task was converted from, so its earlier runs stay readable.
ALTER TABLE relay_tasks
 ADD COLUMN instructions TEXT CHECK(instructions IS NULL OR char_length(instructions) BETWEEN 1 AND 10000),
 ADD COLUMN room TEXT,
 ADD COLUMN precheck TEXT CHECK(precheck IS NULL OR precheck IN ('messages')),
 ADD COLUMN schedule TEXT,
 ADD CONSTRAINT relay_tasks_room FOREIGN KEY(workspace,room) REFERENCES relay_chats(workspace,id);
CREATE INDEX relay_tasks_room ON relay_tasks(workspace,room) WHERE room IS NOT NULL;

-- Minute rules: a fixed period counted from the first run, so an old schedule's
-- interval converts exactly. Calendar rules keep their schema 144 function.
ALTER FUNCTION relay_task_next_due(JSONB,TEXT,BIGINT,BIGINT,TEXT) RENAME TO relay_task_calendar_due;
CREATE FUNCTION relay_task_next_due(rule JSONB, zone TEXT, anchor_ms BIGINT, after_ms BIGINT, w TEXT)
RETURNS BIGINT LANGUAGE plpgsql STABLE AS $$
DECLARE
 period BIGINT;
BEGIN
 IF rule->>'unit'='minute' THEN
  period := GREATEST(COALESCE((rule->>'every')::bigint,15),1)*60000;
  IF after_ms < anchor_ms THEN RETURN anchor_ms; END IF;
  RETURN anchor_ms + (floor((after_ms - anchor_ms)::numeric / period)::bigint + 1) * period;
 END IF;
 RETURN relay_task_calendar_due(rule, zone, anchor_ms, after_ms, w);
END $$;
