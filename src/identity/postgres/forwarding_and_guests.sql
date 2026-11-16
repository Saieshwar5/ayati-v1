-- Schema 125: forwarding a message, and agents invited into one thread.
--
-- A forwarded message carries a copy of the message it forwards: who wrote
-- it, where, when and its text, as it was when forwarded. Editing or deleting
-- the original never changes the copy; deleting the forward removes it.
ALTER TABLE relay_chat_messages ADD COLUMN forwarded JSONB
 CHECK(forwarded IS NULL OR jsonb_typeof(forwarded)='object');

-- In a person's direct chat with their own only-me agent, mentioning another
-- agent invites it into a thread under that message. A guest is never a
-- member of the chat: it reads that thread only, and works there only when the
-- person's own agent hands it a task.
CREATE TABLE relay_thread_guests (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, thread TEXT NOT NULL, agent TEXT NOT NULL,
 invited_by TEXT NOT NULL,
 at BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,chat,thread,agent),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,thread) REFERENCES relay_chat_messages(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_thread_guests_agent ON relay_thread_guests(workspace,chat,agent);
