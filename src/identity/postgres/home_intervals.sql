-- Schema 146: a Home panel's fixed interval ("update every N") runs on the panel's
-- own update queue (relay_home_update_wanted, reason 'interval'), which already
-- handles updates after work and on events. Panel updates are not tasks, so the
-- schedules that ran these intervals stop before schedules become tasks
-- (designs/agent-tasks/one-system.md, decision 5).
ALTER TABLE relay_home_update_wanted
 DROP CONSTRAINT relay_home_update_wanted_reason_check,
 ADD CONSTRAINT relay_home_update_wanted_reason CHECK(reason IN ('work','event','interval'));
ALTER TABLE relay_home_update_runs
 DROP CONSTRAINT relay_home_update_runs_reason_check,
 ADD CONSTRAINT relay_home_update_runs_reason CHECK(reason IN ('work','event','interval'));

-- Each panel whose interval schedule runs keeps its next update time, now on the
-- panel's queue. An update already wanted stays; it books the interval when it starts.
INSERT INTO relay_home_update_wanted(workspace,panel,reason,cause,due_ms)
 SELECT u.workspace,u.panel,'interval','interval',s.next_ms
 FROM relay_home_maintenance u
 JOIN relay_home_schedule_bindings b ON b.workspace=u.workspace AND b.schedule=u.schedule AND b.panel=u.panel
 JOIN relay_agent_schedules s ON s.workspace=u.workspace AND s.id=u.schedule
 WHERE u.enabled AND u.interval_seconds IS NOT NULL AND s.state='enabled' AND s.next_ms IS NOT NULL
ON CONFLICT(workspace,panel) DO NOTHING;
UPDATE relay_home_maintenance u SET schedule=NULL
 FROM relay_agent_schedules s
 WHERE s.workspace=u.workspace AND s.id=u.schedule AND s.state='enabled';

-- No panel schedule runs again. The rows and their bindings stay, so earlier runs
-- stay readable and every list that leaves panel schedules out still does. A blocked
-- or paused one stays linked to its panel, which shows it until its owner reviews.
UPDATE relay_agent_schedules s
 SET state='paused',revision=revision+1,detail='Moved to the Home panel''s own updates.'
 WHERE s.state='enabled'
 AND EXISTS(SELECT 1 FROM relay_home_schedule_bindings b WHERE b.workspace=s.workspace AND b.schedule=s.id);
