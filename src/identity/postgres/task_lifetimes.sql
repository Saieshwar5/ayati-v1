-- Schema 144: tasks with a lifetime (designs/agent-tasks). A task made in chat can
-- repeat on a calendar rule, end on a date or after a number of runs, and keep notes
-- for its next run. Cuckoo books every run; the agent never has to come back by itself.

-- The workspace calendar: India time unless an admin changes it, working days
-- (ISO 1 = Monday) and what a run due on a day off does.
ALTER TABLE relay_workspaces
 ALTER COLUMN timezone SET DEFAULT 'Asia/Kolkata',
 ADD COLUMN working_days SMALLINT[] NOT NULL DEFAULT '{1,2,3,4,5,6}'
  CHECK(cardinality(working_days) BETWEEN 1 AND 7 AND working_days <@ '{1,2,3,4,5,6,7}'::smallint[]),
 ADD COLUMN holiday_rule TEXT NOT NULL DEFAULT 'next' CHECK(holiday_rule IN ('next','before','skip'));
UPDATE relay_workspaces SET timezone='Asia/Kolkata' WHERE timezone IS NULL;

-- Days off the workspace keeps itself; Cuckoo never guesses public holidays.
CREATE TABLE relay_workspace_holidays (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 day DATE NOT NULL,
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 80),
 PRIMARY KEY(workspace,day)
);

-- repeat: the calendar rule (task_rules.rs); timezone: null means the workspace's;
-- anchor_ms: when the rule counts from; until_ms and max_runs end it (or, for a task
-- with done_when, are where it gives up); runs: runs started; paused_*: stopped by a
-- person or by Cuckoo; pieces: parts of a long job continued later.
ALTER TABLE relay_tasks
 ADD COLUMN repeat JSONB CHECK(repeat IS NULL OR jsonb_typeof(repeat)='object'),
 ADD COLUMN timezone TEXT CHECK(timezone IS NULL OR length(timezone) BETWEEN 1 AND 64),
 ADD COLUMN anchor_ms BIGINT,
 ADD COLUMN until_ms BIGINT,
 ADD COLUMN max_runs INT CHECK(max_runs IS NULL OR max_runs BETWEEN 1 AND 1000),
 ADD COLUMN runs INT NOT NULL DEFAULT 0 CHECK(runs>=0),
 ADD COLUMN paused_ms BIGINT,
 ADD COLUMN paused_reason TEXT CHECK(paused_reason IS NULL OR char_length(paused_reason) BETWEEN 1 AND 300),
 ADD COLUMN pieces INT NOT NULL DEFAULT 0 CHECK(pieces>=0),
 ADD CONSTRAINT relay_tasks_repeat_anchor CHECK(repeat IS NULL OR anchor_ms IS NOT NULL);
CREATE INDEX relay_tasks_repeating ON relay_tasks(workspace,agent) WHERE repeat IS NOT NULL AND outcome IS NULL;

-- kind: follow_up (booked by the agent), repeat (a run of the rule), end (closes the
-- task once its last run has finished), continue (the next part of a long job).
-- run: the run number a repeat started. quiet: the run found nothing to report.
-- skipped: Cuckoo did not start it (missed, still running, paused, skipped by a person).
ALTER TABLE relay_task_wakeups
 ADD COLUMN kind TEXT NOT NULL DEFAULT 'follow_up' CHECK(kind IN ('follow_up','repeat','end','continue')),
 ADD COLUMN run INT CHECK(run IS NULL OR run>0),
 ADD COLUMN quiet BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE relay_task_wakeups DROP CONSTRAINT relay_task_wakeups_state_check;
ALTER TABLE relay_task_wakeups ADD CONSTRAINT relay_task_wakeups_state_check
 CHECK(state IN ('pending','started','cancelled','blocked','skipped'));
-- One row per run of a rule, so two controllers can never book or start it twice.
CREATE UNIQUE INDEX relay_task_wakeups_once ON relay_task_wakeups(workspace,task,due_ms) WHERE kind='repeat';
CREATE INDEX relay_task_wakeups_reply ON relay_task_wakeups(workspace,reply) WHERE reply IS NOT NULL;

-- What a run leaves for the next one: one line each, the newest read first.
CREATE TABLE relay_task_notes (
 workspace TEXT NOT NULL, id TEXT NOT NULL, task TEXT NOT NULL,
 note TEXT NOT NULL CHECK(char_length(note) BETWEEN 1 AND 400),
 reply TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,task) REFERENCES relay_tasks(workspace,id)
);
CREATE INDEX relay_task_notes_task ON relay_task_notes(workspace,task,created_ms DESC);

-- Is day d a day off in workspace w?
CREATE FUNCTION relay_workspace_day_off(w TEXT, d DATE) RETURNS BOOLEAN LANGUAGE SQL STABLE AS $$
 SELECT NOT (extract(isodow FROM d)::smallint = ANY(COALESCE(ws.working_days,'{1,2,3,4,5,6}')))
  OR EXISTS(SELECT 1 FROM relay_workspace_holidays h WHERE h.workspace=w AND h.day=d)
 FROM (SELECT (SELECT working_days FROM relay_workspaces WHERE id=w) AS working_days) ws
$$;

-- The first run of `rule` after `after_ms`, in local clock time of `zone`, counting
-- from `anchor_ms`; null when there is none within 400 days. PostgreSQL does the
-- time-zone and daylight-saving arithmetic. A monthly day past the month's end runs
-- on its last day. With working_days, a day off is skipped or moved to the next or
-- previous working day as the workspace chose.
CREATE FUNCTION relay_task_next_due(rule JSONB, zone TEXT, anchor_ms BIGINT, after_ms BIGINT, w TEXT)
RETURNS BIGINT LANGUAGE plpgsql STABLE AS $$
DECLARE
 unit TEXT := rule->>'unit';
 every INT := GREATEST(COALESCE((rule->>'every')::int,1),1);
 at_time TIME := COALESCE((rule->>'at')::time,'09:00');
 from_t TIME := COALESCE((rule->>'from')::time,'00:00');
 to_t TIME := COALESCE((rule->>'to')::time,'23:59');
 working BOOLEAN := COALESCE((rule->>'working_days')::boolean,FALSE);
 anchor_day DATE := (to_timestamp(anchor_ms/1000.0) AT TIME ZONE zone)::date;
 after_ts TIMESTAMPTZ := to_timestamp(after_ms/1000.0);
 first_day DATE := (to_timestamp(after_ms/1000.0) AT TIME ZONE zone)::date - 7;
 days INT[] := ARRAY(SELECT jsonb_array_elements_text(COALESCE(rule->'days','[]'::jsonb))::int);
 dom INT := (rule->>'day_of_month')::int;
 hrule TEXT := COALESCE((SELECT holiday_rule FROM relay_workspaces WHERE id=w),'next');
 d DATE; run_day DATE; last_day DATE; best TIMESTAMPTZ; cand TIMESTAMPTZ; h TIME; matches BOOLEAN;
BEGIN
 IF unit IS NULL OR unit NOT IN ('hour','day','week','month') THEN RETURN NULL; END IF;
 IF unit='week' AND cardinality(days)=0 THEN days := ARRAY[extract(isodow FROM anchor_day)::int]; END IF;
 IF unit='month' AND dom IS NULL THEN dom := extract(day FROM anchor_day)::int; END IF;
 FOR i IN 0..410 LOOP
  d := first_day + i;
  EXIT WHEN best IS NOT NULL AND d > (best AT TIME ZONE zone)::date + 8;
  CONTINUE WHEN d < anchor_day;
  last_day := (date_trunc('month',d) + interval '1 month - 1 day')::date;
  matches := CASE unit
   WHEN 'hour' THEN TRUE
   WHEN 'day' THEN (d - anchor_day) % every = 0
   WHEN 'week' THEN extract(isodow FROM d)::int = ANY(days)
    AND (((d - (extract(isodow FROM d)::int - 1)) - (anchor_day - (extract(isodow FROM anchor_day)::int - 1))) / 7) % every = 0
   ELSE ((extract(year FROM d)::int*12 + extract(month FROM d)::int)
          - (extract(year FROM anchor_day)::int*12 + extract(month FROM anchor_day)::int)) % every = 0
    AND d = CASE WHEN dom = -1 THEN last_day
                 ELSE LEAST(date_trunc('month',d)::date + (dom - 1), last_day) END
  END;
  CONTINUE WHEN NOT matches;
  run_day := d;
  IF working AND relay_workspace_day_off(w, d) THEN
   CONTINUE WHEN hrule='skip';
   FOR j IN 1..14 LOOP
    run_day := CASE WHEN hrule='before' THEN d - j ELSE d + j END;
    EXIT WHEN NOT relay_workspace_day_off(w, run_day);
   END LOOP;
   CONTINUE WHEN relay_workspace_day_off(w, run_day);
  END IF;
  IF unit='hour' THEN
   h := from_t;
   LOOP
    cand := (run_day + h) AT TIME ZONE zone;
    IF cand > after_ts AND (best IS NULL OR cand < best) THEN best := cand; END IF;
    EXIT WHEN h + make_interval(hours => every) > to_t OR h + make_interval(hours => every) <= h;
    h := h + make_interval(hours => every);
   END LOOP;
  ELSE
   cand := (run_day + at_time) AT TIME ZONE zone;
   IF cand > after_ts AND (best IS NULL OR cand < best) THEN best := cand; END IF;
  END IF;
 END LOOP;
 RETURN CASE WHEN best IS NULL THEN NULL ELSE floor(extract(epoch FROM best)*1000)::bigint END;
END $$;

-- A task's status (schema 58) gains paused: stopped by a person or by Cuckoo, with
-- nothing running.
CREATE OR REPLACE FUNCTION relay_task_status(w TEXT, task_id TEXT) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT CASE
  WHEN t.outcome IS NOT NULL THEN t.outcome
  WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=w AND j.task=task_id AND j.state='waiting') THEN 'waiting'
  WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j WHERE j.workspace=w AND j.task=task_id AND j.state<>'finished') THEN 'working'
  WHEN t.paused_ms IS NOT NULL THEN 'paused'
  WHEN t.kind='automatic' THEN COALESCE((SELECT CASE m.state WHEN 'complete' THEN 'done' WHEN 'cancelled' THEN 'cancelled' ELSE 'failed' END
   FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
   WHERE j.workspace=w AND j.task=task_id ORDER BY j.sequence DESC LIMIT 1),'done')
  WHEN (SELECT m.state FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
   WHERE j.workspace=w AND j.task=task_id ORDER BY j.sequence DESC LIMIT 1) IN ('failed','interrupted','access_changed') THEN 'needs_fixing'
  WHEN EXISTS(SELECT 1 FROM relay_task_wakeups k WHERE k.workspace=w AND k.task=task_id AND k.state='blocked'
   AND NOT EXISTS(SELECT 1 FROM relay_task_wakeups l WHERE l.workspace=w AND l.task=task_id AND l.created_ms>k.created_ms)) THEN 'needs_fixing'
  WHEN t.due_ms<(extract(epoch FROM clock_timestamp())*1000)::bigint THEN 'late'
  WHEN t.start_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint
   OR EXISTS(SELECT 1 FROM relay_task_wakeups k WHERE k.workspace=w AND k.task=task_id AND k.state='pending') THEN 'scheduled'
  ELSE 'open' END
 FROM relay_tasks t WHERE t.workspace=w AND t.id=task_id
$$;
