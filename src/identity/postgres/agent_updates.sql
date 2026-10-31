-- An agent's working voice: messages it posts while a request is in progress
-- ("got it, I'll…", a milestone) and the instant "I'm on another request" reply
-- the controller posts when a request queues behind its other work. Each links
-- to the request it is about; none is the request's reply.
ALTER TABLE relay_chat_messages ADD COLUMN update_of TEXT,
 ADD CONSTRAINT relay_message_update_of FOREIGN KEY(workspace,update_of) REFERENCES relay_chat_messages(workspace,id);
CREATE INDEX relay_messages_updates ON relay_chat_messages(workspace,update_of) WHERE update_of IS NOT NULL;
