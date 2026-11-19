-- Schema 139: a member's assistant helps them set up an agent.
--
-- One row per turn of the setup chat in New agent or on an agent's Instructions
-- tab. The member's assistant is asked in its own room and hands back the parts
-- it drafted (ground rules, skills, apps the job needs) with write_agent_setup.
-- Nothing here reaches an agent: the member reviews the parts and saves them
-- through the ordinary agent and skill routes.
CREATE TABLE relay_agent_setups (
 -- The assistant's reply this turn's parts belong to.
 workspace TEXT NOT NULL, reply TEXT NOT NULL, user_id TEXT NOT NULL,
 -- What the assistant is shown (shown) and what the checks need afterwards:
 -- the agent's name, a saved agent's id, the names of the drafts the member
 -- already had and whether ground rules may be drafted.
 request JSONB NOT NULL,
 parts JSONB NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(parts)='array'),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,reply),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
