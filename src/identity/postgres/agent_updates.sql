-- An agent's working voice: messages it posts while a request is in progress
-- ("got it, I'll…", a milestone) and the instant "I'm on another request" reply
-- the controller posts when a request queues behind its other work. Each links
-- to the request it is about; none is the request's reply.
ALTER TABLE relay_chat_messages ADD COLUMN update_of TEXT,
 ADD CONSTRAINT relay_message_update_of FOREIGN KEY(workspace,update_of) REFERENCES relay_chat_messages(workspace,id);
CREATE INDEX relay_messages_updates ON relay_chat_messages(workspace,update_of) WHERE update_of IS NOT NULL;

-- The front desk: a short, tool-free model turn that answers a person at once
-- while the agent's work lane is busy. One row per request it spoke for.
CREATE TABLE relay_front_desk (
 id TEXT PRIMARY KEY REFERENCES relay_executor_tasks(id),
 workspace TEXT NOT NULL, reply TEXT NOT NULL,
 prepared JSONB CHECK(octet_length(prepared::text)<=65536),
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','finished')),
 decision TEXT CHECK(decision IN ('acknowledged','answered','nothing')),
 said TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 finished_ms BIGINT,
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id)
);
ALTER TABLE relay_executor_tasks DROP CONSTRAINT relay_executor_tasks_purpose_check,
 ADD CONSTRAINT relay_executor_tasks_purpose_check CHECK(purpose IN ('sample','setup_suggestion','front_desk'));
