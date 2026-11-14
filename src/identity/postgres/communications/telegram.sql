-- Telegram (schema 113). Cuckoo runs one manager bot. A workspace admin
-- connects Telegram once and becomes its Telegram owner; each agent's own bot
-- is then created by that owner with one tap in Telegram, as a bot Cuckoo's
-- manager bot manages. Bot tokens are never stored: the manager bot asks
-- Telegram for a managed bot's token when it is needed.

-- The admin who connected the workspace's Telegram. Their Telegram account
-- creates the agents' bots.
CREATE TABLE relay_telegram_workspaces (
 workspace TEXT PRIMARY KEY REFERENCES relay_workspaces(id),
 owner TEXT NOT NULL REFERENCES relay_users(id),
 connected BIGINT NOT NULL
);

-- A person's Telegram account, linked with a Start code from their signed-in
-- Cuckoo session. One person, one account; an account belongs to one person.
CREATE TABLE relay_user_telegram (
 user_id TEXT PRIMARY KEY REFERENCES relay_users(id),
 telegram_id BIGINT NOT NULL UNIQUE CHECK(telegram_id > 0),
 username TEXT CHECK(username IS NULL OR username ~ '^[A-Za-z0-9_]{1,64}$'),
 linked BIGINT NOT NULL
);
-- Every Telegram account ever linked, and to whom. An account is never moved to
-- another person, even after it was unlinked.
CREATE TABLE relay_telegram_accounts (
 telegram_id BIGINT PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES relay_users(id)
);

-- Short-lived Start codes; only their digest is kept. A code made while
-- connecting a workspace's Telegram also makes its maker the Telegram owner.
CREATE TABLE relay_telegram_link_codes (
 digest TEXT PRIMARY KEY,
 user_id TEXT NOT NULL UNIQUE REFERENCES relay_users(id),
 workspace TEXT REFERENCES relay_workspaces(id),
 expires BIGINT NOT NULL
);

-- Each agent's bot. While `requested`, Cuckoo waits for the owner to create it
-- in Telegram. A bot serves one agent forever: switching Telegram off and on
-- brings the same bot back, and no other agent can ever have it.
CREATE TABLE relay_telegram_bots (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 agent TEXT NOT NULL,
 state TEXT NOT NULL CHECK(state IN ('requested','active','off')),
 bot_id BIGINT UNIQUE CHECK(bot_id IS NULL OR bot_id > 0),
 username TEXT CHECK(username IS NULL OR username ~ '^[A-Za-z0-9_]{5,64}$'),
 endpoint TEXT,
 -- The Telegram account asked to create the bot, and the request's details.
 requested_by BIGINT,
 request_id INTEGER,
 suggested TEXT,
 requested BIGINT,
 -- Where this bot's updates were read up to, and which controller reads them.
 next_update BIGINT NOT NULL DEFAULT 0,
 lease_until BIGINT NOT NULL DEFAULT 0,
 changed BIGINT NOT NULL,
 PRIMARY KEY(workspace,agent),
 CHECK((state='requested') = (bot_id IS NULL)),
 CHECK((bot_id IS NULL) = (endpoint IS NULL)),
 FOREIGN KEY(workspace,endpoint) REFERENCES relay_communication_endpoints(workspace,id)
);
CREATE INDEX relay_telegram_bots_requests ON relay_telegram_bots(requested_by) WHERE state='requested';

-- Where Cuckoo's manager bot was read up to, and which controller reads it.
CREATE TABLE relay_telegram_manager (
 id INTEGER PRIMARY KEY CHECK(id = 1),
 next_update BIGINT NOT NULL DEFAULT 0,
 lease_until BIGINT NOT NULL DEFAULT 0
);
INSERT INTO relay_telegram_manager(id) VALUES(1);
