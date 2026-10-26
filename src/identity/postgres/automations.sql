-- Shared routines retain the original authorizing person, independent of membership.
CREATE TABLE relay_agent_schedules (
 workspace TEXT NOT NULL, id TEXT NOT NULL, agent TEXT NOT NULL, principal TEXT NOT NULL,
 chat TEXT NOT NULL, name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 100),
 content TEXT NOT NULL CHECK(octet_length(content) BETWEEN 1 AND 10000),
 first_ms BIGINT NOT NULL CHECK(first_ms>0),
 interval_seconds BIGINT CHECK(interval_seconds BETWEEN 1 AND 31536000),
 timezone TEXT NOT NULL CHECK(length(timezone) BETWEEN 1 AND 100),
 request_hash TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'enabled' CHECK(state IN ('enabled','paused','blocked','completed')),
 revision BIGINT NOT NULL DEFAULT 1,
 next_ms BIGINT, checked_ms BIGINT NOT NULL DEFAULT 0, detail TEXT NOT NULL DEFAULT '',
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 FOREIGN KEY(principal) REFERENCES relay_users(id),
 CHECK((state='completed')=(next_ms IS NULL))
);
CREATE INDEX relay_schedule_due ON relay_agent_schedules(checked_ms,next_ms,created_ms,id) WHERE state='enabled';
CREATE INDEX relay_schedule_agent ON relay_agent_schedules(workspace,agent,created_ms DESC,id);
CREATE TABLE relay_schedule_changes (
 workspace TEXT NOT NULL, schedule TEXT NOT NULL, request TEXT NOT NULL,
 actor TEXT NOT NULL REFERENCES relay_users(id), payload JSONB NOT NULL, result JSONB NOT NULL,
 PRIMARY KEY(workspace,schedule,request),
 FOREIGN KEY(workspace,schedule) REFERENCES relay_agent_schedules(workspace,id)
);
CREATE TABLE relay_schedule_occurrences (
 workspace TEXT NOT NULL, schedule TEXT NOT NULL, due_ms BIGINT NOT NULL,
 message TEXT NOT NULL UNIQUE, reply TEXT NOT NULL UNIQUE,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,schedule,due_ms),
 FOREIGN KEY(workspace,schedule) REFERENCES relay_agent_schedules(workspace,id),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_jobs(workspace,reply)
);
CREATE FUNCTION relay_schedule_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.workspace,NEW.id,NEW.agent,NEW.principal,NEW.chat,NEW.name,NEW.content,NEW.first_ms,
        NEW.interval_seconds,NEW.timezone,NEW.request_hash,NEW.created_ms)
  IS DISTINCT FROM ROW(OLD.workspace,OLD.id,OLD.agent,OLD.principal,OLD.chat,OLD.name,OLD.content,OLD.first_ms,
        OLD.interval_seconds,OLD.timezone,OLD.request_hash,OLD.created_ms) THEN
  RAISE EXCEPTION 'A saved routine configuration cannot change';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_schedule_immutable BEFORE UPDATE ON relay_agent_schedules
 FOR EACH ROW EXECUTE FUNCTION relay_schedule_immutable();
CREATE FUNCTION relay_schedule_principal_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND NEW.role IS NOT DISTINCT FROM OLD.role THEN RETURN NEW; END IF;
 UPDATE relay_agent_schedules SET state='blocked',revision=revision+1,
  detail='Authorizing workspace membership changed. The original person must review and resume.'
 WHERE workspace=OLD.workspace AND principal=OLD.user_id AND state='enabled';
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_schedule_principal_changed AFTER DELETE OR UPDATE OF role ON relay_members
 FOR EACH ROW EXECUTE FUNCTION relay_schedule_principal_changed();
