-- One message may ask up to three agents (2026-11-07), each with its own reply
-- and job: jobs are unique per asking message and agent, not per message.
-- The expression index starts with the workspace (D226 rule 1).
CREATE UNIQUE INDEX relay_jobs_request_agent
 ON relay_chat_jobs(workspace,request_message,(payload->>'agent'))
 WHERE request_message IS NOT NULL;
ALTER TABLE relay_chat_jobs DROP CONSTRAINT relay_chat_jobs_request_message_key;
