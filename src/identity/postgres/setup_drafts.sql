CREATE TABLE relay_control_drafts (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, user_id TEXT NOT NULL REFERENCES relay_users(id),
 id TEXT NOT NULL, revision BIGINT NOT NULL DEFAULT 1, data JSONB NOT NULL,
 state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','activated')),
 preview_revision BIGINT, receipt JSONB,
 created BIGINT GENERATED ALWAYS AS IDENTITY,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id)
);
CREATE INDEX relay_control_drafts_person ON relay_control_drafts(workspace,agent,user_id,created DESC);
