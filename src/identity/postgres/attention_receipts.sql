-- These records carry no request, approval, dispatch or outcome authority.
CREATE TABLE relay_attention_receipts (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('question','approval','outcome')),
 source TEXT NOT NULL, version TEXT NOT NULL CHECK(length(version) BETWEEN 1 AND 96),
 read_ms BIGINT NOT NULL, reviewed_ms BIGINT,
 PRIMARY KEY(workspace,user_id,kind,source),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE,
 CHECK(reviewed_ms IS NULL OR kind='outcome')
);
CREATE INDEX relay_attention_questions ON relay_task_questions(workspace,created_ms DESC,id DESC)
 WHERE state='pending';
CREATE INDEX relay_attention_actions ON relay_control_actions(workspace,approver,updated DESC,id DESC)
 WHERE state IN ('pending','executing','uncertain');
CREATE INDEX relay_attention_outcomes ON relay_chat_jobs(workspace,(payload->>'requester'),finished_ms DESC,reply DESC)
 WHERE state='finished';
