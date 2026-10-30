-- A message may quote one earlier message of the same conversation (C4).
ALTER TABLE relay_chat_messages ADD COLUMN reply_to TEXT;
ALTER TABLE relay_chat_messages ADD CONSTRAINT relay_message_reply_to
 FOREIGN KEY(workspace,reply_to) REFERENCES relay_chat_messages(workspace,id);
