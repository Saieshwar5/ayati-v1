-- Schema 119: bots post into a channel through a Slack-compatible webhook.
-- A bot is its own kind of author: never a person, never an agent, so it
-- holds no membership, asks no agent and approves nothing.
ALTER TABLE relay_chat_messages DROP CONSTRAINT relay_chat_messages_kind_check;
ALTER TABLE relay_chat_messages ADD CONSTRAINT relay_chat_messages_kind_check CHECK(kind IN ('user','agent','bot'));
CREATE TABLE relay_chat_bots (
 workspace TEXT NOT NULL,
 id TEXT NOT NULL,
 chat TEXT NOT NULL,
 name TEXT NOT NULL CHECK(char_length(name) BETWEEN 1 AND 60),
 -- The webhook's secret, stored only as its SHA-256.
 token_hash TEXT NOT NULL UNIQUE,
 created_by TEXT NOT NULL,
 created BIGINT NOT NULL,
 enabled BOOLEAN NOT NULL DEFAULT TRUE,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE INDEX relay_chat_bots_by_chat ON relay_chat_bots(workspace,chat);
