-- Subjects (schema 118, C3 of the connected systems plan, D249). An event's
-- `subject` names one user of the outside system: a student, a patient, an
-- order. Automations may keep a private room per subject, events for one
-- subject run in order while different subjects run side by side, and tools
-- marked "Needs the subject" get it from the event, never from the agent.
ALTER TABLE relay_event_triggers DROP CONSTRAINT relay_event_triggers_memory_check;
ALTER TABLE relay_event_triggers ADD CONSTRAINT relay_event_triggers_memory_check
 CHECK(memory IN ('fresh','remember','subject'));
ALTER TABLE relay_event_deliveries ADD COLUMN subject TEXT
 CHECK(subject IS NULL OR length(subject) BETWEEN 1 AND 128);
CREATE INDEX relay_event_subject_pending ON relay_event_deliveries(workspace,trigger_id,subject) WHERE state='pending';
-- One room per agent, system and subject, shared by that system's automations.
CREATE TABLE relay_subject_rooms (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, connection TEXT NOT NULL,
 subject TEXT NOT NULL CHECK(length(subject) BETWEEN 1 AND 128),
 room TEXT NOT NULL,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,agent,connection,subject),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id),
 FOREIGN KEY(workspace,room) REFERENCES relay_chats(workspace,id)
);
