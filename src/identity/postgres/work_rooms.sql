-- Work rooms (schema 56). A schedule's runs happen in its own private room,
-- never in a conversation people use: it holds the person who set it up and
-- the agent, never appears in Chat, and is the schedule's memory of earlier
-- runs. The schedule's former conversation becomes an optional place to post
-- one short result message.
ALTER TABLE relay_chats ADD COLUMN work BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE relay_chats ADD CONSTRAINT relay_chats_work_private CHECK(NOT work OR (kind='direct' AND visibility='private' AND direct_key IS NULL));
ALTER TABLE relay_agent_schedules ALTER COLUMN chat DROP NOT NULL;
ALTER TABLE relay_agent_schedules ADD COLUMN room TEXT;
ALTER TABLE relay_agent_schedules ADD CONSTRAINT relay_agent_schedules_room FOREIGN KEY(workspace,room) REFERENCES relay_chats(workspace,id);

-- One short result message per run, posted where the schedule says, or why not.
CREATE TABLE relay_run_posts (
 workspace TEXT NOT NULL, reply TEXT NOT NULL, chat TEXT NOT NULL,
 message TEXT, state TEXT NOT NULL CHECK(state IN ('posted','skipped')),
 reason TEXT CHECK(reason IS NULL OR length(reason)<=300),
 created BIGINT NOT NULL,
 PRIMARY KEY(workspace,reply),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id),
 CHECK((state='posted')=(message IS NOT NULL))
);
