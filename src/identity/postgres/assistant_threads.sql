-- Schema 128: assistant threads.
--
-- A thread is the permission a member gives their assistant to talk to one
-- agent or person for them about one matter. Its messages are ordinary
-- messages from the member in their direct chat with that agent or person.
-- `assisted` records, for the member and admins only, that the assistant
-- wrote a message, in which thread, and who sent it.
ALTER TABLE relay_chat_messages ADD COLUMN assisted JSONB
 CHECK(assisted IS NULL OR jsonb_typeof(assisted)='object');

CREATE TABLE relay_work_threads (
 workspace TEXT NOT NULL, id TEXT NOT NULL, owner TEXT NOT NULL, agent TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('agent','user')), counterpart TEXT NOT NULL,
 title TEXT NOT NULL, opening TEXT NOT NULL, chat TEXT,
 -- Who proposed it: the assistant (the member pressed Start) or the member.
 origin TEXT NOT NULL CHECK(origin IN ('assistant','owner')),
 state TEXT NOT NULL CHECK(state IN ('suggested','running','stopped','deleted')),
 detail TEXT NOT NULL DEFAULT '', sent INTEGER NOT NULL DEFAULT 0,
 -- The newest message of the other side the assistant was given, and its run.
 handled BIGINT NOT NULL DEFAULT 0, job TEXT,
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 updated_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,owner) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE
);
-- One running thread per member and chat: a reply there belongs to one matter.
CREATE UNIQUE INDEX relay_work_threads_running ON relay_work_threads(workspace,owner,chat) WHERE state='running';
CREATE INDEX relay_work_threads_owner ON relay_work_threads(workspace,owner,updated_ms DESC);

-- A retried send posts once.
CREATE TABLE relay_work_thread_sends (
 workspace TEXT NOT NULL, thread TEXT NOT NULL, key TEXT NOT NULL, message TEXT NOT NULL,
 PRIMARY KEY(workspace,thread,key),
 FOREIGN KEY(workspace,thread) REFERENCES relay_work_threads(workspace,id) ON DELETE CASCADE
);

-- The other person's answer wakes the scheduler, which hands it to the
-- assistant (an agent's answer already does, when its job finishes).
CREATE INDEX relay_work_threads_chat ON relay_work_threads(workspace,chat) WHERE state='running';
CREATE FUNCTION relay_announce_thread_message() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF EXISTS(SELECT 1 FROM relay_work_threads t WHERE t.workspace=NEW.workspace AND t.chat=NEW.chat AND t.state='running') THEN
  PERFORM pg_notify('relay_work', current_schema() || '/schedule');
 END IF;
 RETURN NULL;
END $$;
CREATE TRIGGER relay_thread_message_announced AFTER INSERT ON relay_chat_messages
 FOR EACH ROW WHEN (NEW.kind='user') EXECUTE FUNCTION relay_announce_thread_message();
