-- Each person chooses how much a conversation may notify them. No row means the
-- default: every message in direct messages, @mentions and replies in channels.
CREATE TABLE relay_chat_notify (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL, chat TEXT NOT NULL,
 level TEXT NOT NULL CHECK(level IN ('all','mentions','none')),
 PRIMARY KEY(workspace,user_id,chat),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE
);
