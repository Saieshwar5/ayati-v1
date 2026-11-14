-- Automation memory (schema 115, E2 of how work reaches agents). Every event
-- trigger now runs in a private work room, as schedules have since schema 56:
-- never in a conversation people use. A trigger's conversation becomes an
-- optional place to post a short result. Each automation says what it
-- remembers: 'fresh' gives every run its own new room, so a run sees nothing
-- from earlier runs; 'remember' keeps one room, so runs see the earlier ones.
ALTER TABLE relay_event_triggers ALTER COLUMN chat DROP NOT NULL;
ALTER TABLE relay_event_triggers ADD COLUMN memory TEXT NOT NULL DEFAULT 'fresh'
 CHECK(memory IN ('fresh','remember'));
ALTER TABLE relay_event_triggers ADD COLUMN room TEXT;
ALTER TABLE relay_event_triggers ADD CONSTRAINT relay_event_triggers_room
 FOREIGN KEY(workspace,room) REFERENCES relay_chats(workspace,id);
-- The room a delivery ran in; older deliveries ran in the trigger's conversation.
ALTER TABLE relay_event_deliveries ADD COLUMN room TEXT;
ALTER TABLE relay_event_deliveries ADD CONSTRAINT relay_event_deliveries_room
 FOREIGN KEY(workspace,room) REFERENCES relay_chats(workspace,id);
-- Posted summaries are looked up by message to hide or leave them out with their run.
CREATE INDEX relay_run_posts_message ON relay_run_posts(workspace,message) WHERE message IS NOT NULL;
-- Schedules keep remembering earlier runs, as they did; 'fresh' is a choice.
ALTER TABLE relay_agent_schedules ADD COLUMN memory TEXT NOT NULL DEFAULT 'remember'
 CHECK(memory IN ('fresh','remember'));

-- Memory is part of a trigger's saved configuration.
CREATE OR REPLACE FUNCTION relay_event_trigger_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF ROW(NEW.workspace,NEW.id,NEW.agent,NEW.principal,NEW.source,NEW.binding,NEW.source_definition,
 NEW.connection,NEW.read_operation,NEW.chat,NEW.input,NEW.request_hash,NEW.created_ms,NEW.memory)
 IS DISTINCT FROM ROW(OLD.workspace,OLD.id,OLD.agent,OLD.principal,OLD.source,OLD.binding,OLD.source_definition,
 OLD.connection,OLD.read_operation,OLD.chat,OLD.input,OLD.request_hash,OLD.created_ms,OLD.memory) THEN
  RAISE EXCEPTION 'A saved event trigger configuration cannot change';
 END IF; RETURN NEW;
END $$;
