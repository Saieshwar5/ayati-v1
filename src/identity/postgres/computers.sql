-- Schema 103: agent computers (runtime/agent-computers.md). One computer per agent;
-- its compute runs only while the agent works. People never see the provider.
CREATE TABLE relay_computers (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 agent TEXT NOT NULL,
 size TEXT NOT NULL CHECK(size IN ('small','standard','large')),
 provider TEXT NOT NULL CHECK(char_length(provider) BETWEEN 1 AND 40),
 image TEXT NOT NULL CHECK(char_length(image) BETWEEN 1 AND 120),
 -- The provider's id for it; empty until it is first created.
 provider_id TEXT CHECK(char_length(provider_id) <= 200),
 state TEXT NOT NULL DEFAULT 'asleep' CHECK(state IN ('asleep','awake','deleting')),
 -- The network rule last applied, so a changed setting is applied on next use.
 internet TEXT NOT NULL DEFAULT '' CHECK(char_length(internet) <= 9000),
 -- One request uses the computer at a time; a hold lapses a few minutes after its last use.
 held_by TEXT,
 held_until BIGINT NOT NULL DEFAULT 0,
 awake_since BIGINT,
 last_used BIGINT NOT NULL DEFAULT 0,
 created_at BIGINT NOT NULL,
 problem TEXT CHECK(char_length(problem) <= 500),
 PRIMARY KEY(workspace, agent)
);
CREATE INDEX relay_computers_awake ON relay_computers(state, last_used) WHERE state<>'asleep';

-- Each stretch a computer was awake: what it was charged and what it cost.
CREATE TABLE relay_computer_sessions (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
 id BIGSERIAL,
 agent TEXT NOT NULL,
 reply TEXT,
 person TEXT,
 size TEXT NOT NULL,
 provider TEXT NOT NULL,
 woke_at BIGINT NOT NULL,
 slept_at BIGINT,
 seconds BIGINT NOT NULL DEFAULT 0 CHECK(seconds >= 0),
 charge_micro BIGINT NOT NULL DEFAULT 0 CHECK(charge_micro >= 0),
 cost_micro BIGINT NOT NULL DEFAULT 0 CHECK(cost_micro >= 0),
 PRIMARY KEY(workspace, id)
);
CREATE INDEX relay_computer_sessions_agent ON relay_computer_sessions(workspace, agent, woke_at DESC);
CREATE INDEX relay_computer_sessions_open ON relay_computer_sessions(workspace, agent) WHERE slept_at IS NULL;

-- What the agent did on its computer for a request: shown in Activity and the live view.
CREATE TABLE relay_computer_steps (
 workspace TEXT NOT NULL,
 reply TEXT NOT NULL,
 position INTEGER NOT NULL,
 agent TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('command','list','read','write')),
 detail TEXT NOT NULL CHECK(char_length(detail) <= 4000),
 state TEXT NOT NULL CHECK(state IN ('completed','failed','denied','pending')),
 exit_code INTEGER,
 output TEXT CHECK(char_length(output) <= 140000),
 output_bytes BIGINT NOT NULL DEFAULT 0,
 milliseconds BIGINT NOT NULL DEFAULT 0,
 at BIGINT NOT NULL,
 PRIMARY KEY(workspace, reply, position),
 FOREIGN KEY(workspace, reply) REFERENCES relay_chat_messages(workspace, id) ON DELETE CASCADE
);
