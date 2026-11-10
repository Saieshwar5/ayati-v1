-- Agents' email addresses (schema 112). Cuckoo receives all mail for its mail
-- domain; activating email for an agent saves a new address on that domain as
-- the agent's live endpoint. Addresses are never given to another agent.
CREATE TABLE relay_email_addresses (
 address TEXT PRIMARY KEY CHECK(address = lower(address) AND length(address) <= 254 AND address ~ '^[a-z0-9][a-z0-9-]*@[a-z0-9.-]+$'),
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 agent TEXT NOT NULL,
 endpoint TEXT NOT NULL,
 -- A released address keeps its agent, which gets it back if email is
 -- switched on again.
 state TEXT NOT NULL CHECK(state IN ('assigned','released')),
 changed BIGINT NOT NULL,
 UNIQUE(workspace,endpoint),
 FOREIGN KEY(workspace,endpoint) REFERENCES relay_communication_endpoints(workspace,id)
);
-- One address per agent.
CREATE UNIQUE INDEX relay_email_addresses_agent ON relay_email_addresses(workspace,agent) WHERE state='assigned';

-- How a reply joins the thread of an admitted email. Kept only for admitted
-- mail, beside its inbox receipt.
CREATE TABLE relay_email_messages (
 workspace TEXT NOT NULL,
 endpoint TEXT NOT NULL,
 event TEXT NOT NULL,
 message_id TEXT NOT NULL CHECK(length(message_id) BETWEEN 3 AND 252),
 reference_ids JSONB NOT NULL CHECK(jsonb_typeof(reference_ids)='array' AND jsonb_array_length(reference_ids) BETWEEN 1 AND 20),
 subject TEXT NOT NULL CHECK(length(subject) <= 800),
 PRIMARY KEY(workspace,endpoint,event),
 FOREIGN KEY(workspace,endpoint,event) REFERENCES relay_communication_inbox(workspace,endpoint,event)
);
