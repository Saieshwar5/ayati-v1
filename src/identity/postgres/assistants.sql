-- Schema 127: every member has one assistant.
--
-- An assistant is the member's own private agent: only they talk to it, it
-- joins no channel and has no email address or other owners. It is made for
-- them the first time they open Chat. The row is written before the agent, so
-- two requests at once agree on one agent id.
CREATE TABLE relay_assistants (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL, agent TEXT NOT NULL,
 at BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,user_id), UNIQUE(workspace,agent),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
