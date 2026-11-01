-- Shared rooms (schema 55). A question asked in a shared room whose answer is
-- private to the asker moves to their direct conversation with the agent. The
-- moved request is written by the agent; this records whose question it was.
CREATE TABLE relay_private_continuations (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 source TEXT NOT NULL,
 source_message TEXT NOT NULL,
 chat TEXT NOT NULL,
 message TEXT NOT NULL,
 reply TEXT NOT NULL,
 created BIGINT NOT NULL,
 PRIMARY KEY(workspace,source),
 UNIQUE(workspace,message),
 UNIQUE(workspace,reply),
 FOREIGN KEY(workspace,source) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,source_message) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
