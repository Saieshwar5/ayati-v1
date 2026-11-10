-- WhatsApp numbers Cuckoo provides (schema 111). Cuckoo registers numbers with
-- its WhatsApp provider ahead of time; activating WhatsApp for an agent takes one
-- from this pool and saves it as the agent's live endpoint. The operator file
-- lists which numbers exist; this table records what each one is doing.
CREATE TABLE relay_whatsapp_numbers (
 number TEXT PRIMARY KEY CHECK(number ~ '^\+[1-9][0-9]{6,14}$'),
 state TEXT NOT NULL CHECK(state IN ('ready','assigned','cooling')),
 -- The agent it serves or last served. A cooling number keeps them, so the same
 -- agent gets its number back if WhatsApp is switched on again while it rests.
 workspace TEXT REFERENCES relay_workspaces(id),
 agent TEXT,
 endpoint TEXT,
 changed BIGINT NOT NULL,
 -- A released number rests until then, so people who saved it can't reach
 -- another agent with it.
 ready_after BIGINT NOT NULL DEFAULT 0,
 CHECK((state='ready') = (endpoint IS NULL)),
 CHECK((workspace IS NULL) = (endpoint IS NULL) AND (agent IS NULL) = (endpoint IS NULL)),
 FOREIGN KEY(workspace,endpoint) REFERENCES relay_communication_endpoints(workspace,id)
);
-- One WhatsApp number per agent.
CREATE UNIQUE INDEX relay_whatsapp_numbers_agent ON relay_whatsapp_numbers(workspace,agent) WHERE state='assigned';

-- A person's own WhatsApp number, verified by a code they sent from it. One
-- person, one number; a number belongs to one person.
CREATE TABLE relay_user_mobiles (
 user_id TEXT PRIMARY KEY REFERENCES relay_users(id),
 number TEXT NOT NULL UNIQUE CHECK(number ~ '^\+[1-9][0-9]{6,14}$'),
 verified BIGINT NOT NULL
);

-- A short-lived code a person sends from WhatsApp to prove the number is theirs.
-- Only its digest is kept.
CREATE TABLE relay_mobile_link_codes (
 digest TEXT PRIMARY KEY,
 user_id TEXT NOT NULL UNIQUE REFERENCES relay_users(id),
 expires BIGINT NOT NULL
);
