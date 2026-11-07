-- Schema 96: announce work instead of polling for it (see work_signals.rs).
-- Row triggers fire only for rows that change, so a recovery statement that
-- matches nothing announces nothing. The dispatcher's own claims (to 'running')
-- and offers are not announced, so a dispatch round never wakes itself.
CREATE FUNCTION relay_announce_work() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE kind text;
BEGIN
 FOREACH kind IN ARRAY TG_ARGV LOOP
  PERFORM pg_notify('relay_work', current_schema() || '/' || kind);
 END LOOP;
 RETURN NULL;
END $$;

-- New, returned or finished work frees capacity, an agent or a queue place.
CREATE TRIGGER relay_job_added_work AFTER INSERT ON relay_chat_jobs
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('dispatch','schedule');
CREATE TRIGGER relay_job_moved_work AFTER UPDATE OF state ON relay_chat_jobs
 FOR EACH ROW WHEN (NEW.state<>OLD.state AND NEW.state<>'running')
 EXECUTE FUNCTION relay_announce_work('dispatch','schedule');
CREATE TRIGGER relay_task_added_work AFTER INSERT ON relay_executor_tasks
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('dispatch');
CREATE TRIGGER relay_task_moved_work AFTER UPDATE OF state ON relay_executor_tasks
 FOR EACH ROW WHEN (NEW.state<>OLD.state AND NEW.state<>'running')
 EXECUTE FUNCTION relay_announce_work('dispatch');

-- A slot became ready: a new one, a new generation, or prepared at last.
-- An unchanged heartbeat writes no slot row (executor_slots.rs).
CREATE TRIGGER relay_slot_added_work AFTER INSERT ON relay_executor_slots
 FOR EACH ROW WHEN (NEW.state='ready') EXECUTE FUNCTION relay_announce_work('dispatch');
CREATE TRIGGER relay_slot_ready_work AFTER UPDATE OF state,generation ON relay_executor_slots
 FOR EACH ROW WHEN (NEW.state='ready' AND (OLD.state<>'ready' OR OLD.generation<>NEW.generation))
 EXECUTE FUNCTION relay_announce_work('dispatch');

-- A queued recording for any controller's worker; a finished one releases the
-- request that waited for it (pool_candidate.sql).
CREATE TRIGGER relay_transcript_added_work AFTER INSERT ON relay_object_transcripts
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('transcribe');
CREATE TRIGGER relay_transcript_done_work AFTER UPDATE OF status ON relay_object_transcripts
 FOR EACH ROW WHEN (OLD.status='pending' AND NEW.status<>'pending')
 EXECUTE FUNCTION relay_announce_work('dispatch');

-- What the scheduler admits, and the times it must wake for.
CREATE TRIGGER relay_schedule_work AFTER INSERT OR UPDATE OF state,next_ms ON relay_agent_schedules
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');
CREATE TRIGGER relay_event_work AFTER INSERT ON relay_event_deliveries
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');
CREATE TRIGGER relay_arrival_work AFTER INSERT ON relay_library_arrivals
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');
CREATE TRIGGER relay_app_event_work AFTER INSERT ON relay_built_app_events
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');
CREATE TRIGGER relay_wakeup_work AFTER INSERT OR UPDATE OF state,due_ms ON relay_task_wakeups
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');
CREATE TRIGGER relay_panel_update_work AFTER INSERT OR UPDATE OF due_ms ON relay_home_update_wanted
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');
CREATE TRIGGER relay_handoff_work AFTER INSERT ON relay_agent_handoffs
 FOR EACH ROW EXECUTE FUNCTION relay_announce_work('schedule');

-- Recovery reads only unfinished rows, however much history accumulates
-- (checked with EXPLAIN on 100,000 finished attempts). Almost every capability
-- is revoked (expires=0) when its slot advances.
CREATE INDEX relay_credentials_unrevoked ON relay_assignment_credentials(assignment)
 WHERE expires<>0;
CREATE INDEX relay_front_desk_queued ON relay_front_desk(id) WHERE state='queued';
CREATE INDEX relay_executors_live ON relay_executors(heartbeat)
 WHERE pooled AND state IN ('serving','draining');
