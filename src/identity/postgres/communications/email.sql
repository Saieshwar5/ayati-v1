-- Agent email (schema 113, E4). An agent's address on a Cuckoo mail domain is
-- an ordinary live endpoint (`email`, server `ses`). It is given once and never
-- to another agent, so people who saved it never reach someone else's agent.
-- Switching email off keeps the address for the same agent.
CREATE TABLE relay_email_addresses (
 address TEXT PRIMARY KEY CHECK(address = lower(address) AND length(address) BETWEEN 3 AND 254
   AND address ~ '^[a-z0-9][a-z0-9.-]{0,63}@[a-z0-9][a-z0-9.-]*$'),
 domain TEXT NOT NULL CHECK(length(domain) BETWEEN 3 AND 200),
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 agent TEXT NOT NULL,
 endpoint TEXT NOT NULL,
 created BIGINT NOT NULL,
 CHECK(address LIKE '%@' || domain),
 UNIQUE(workspace,agent,domain),
 FOREIGN KEY(workspace,endpoint) REFERENCES relay_communication_endpoints(workspace,id)
);
CREATE TRIGGER relay_email_address_immutable BEFORE UPDATE OR DELETE ON relay_email_addresses
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();

-- Each mail an agent took in: the thread it belongs to in the person's direct
-- room, and what a reply needs to stay in the person's mail thread. Only mail
-- that was admitted is kept; held and ignored mail leaves no headers behind.
CREATE TABLE relay_email_messages (
 workspace TEXT NOT NULL,
 endpoint TEXT NOT NULL,
 event TEXT NOT NULL,
 -- The verified sender, so a thread is only ever continued by its own person.
 sender TEXT NOT NULL,
 message_id TEXT CHECK(message_id IS NULL OR length(message_id) <= 300),
 subject TEXT NOT NULL CHECK(length(subject) <= 200),
 -- References then In-Reply-To, oldest first (at most 20).
 parents JSONB NOT NULL CHECK(jsonb_typeof(parents)='array' AND jsonb_array_length(parents) <= 20),
 -- The person's message in the room, and the first message of its mail thread.
 chat_message TEXT NOT NULL,
 thread_root TEXT NOT NULL,
 PRIMARY KEY(workspace,endpoint,event),
 FOREIGN KEY(workspace,endpoint,event) REFERENCES relay_communication_inbox(workspace,endpoint,event),
 FOREIGN KEY(workspace,chat_message) REFERENCES relay_chat_messages(workspace,id),
 FOREIGN KEY(workspace,thread_root) REFERENCES relay_chat_messages(workspace,id)
);
CREATE INDEX relay_email_messages_message_id ON relay_email_messages(workspace,endpoint,message_id);
CREATE TRIGGER relay_email_message_immutable BEFORE UPDATE OR DELETE ON relay_email_messages
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();
-- Replies are found again by SES's ID when a person answers one.
CREATE INDEX relay_communication_outbox_provider ON relay_communication_outbox(workspace,endpoint,provider_id)
 WHERE provider_id IS NOT NULL;
