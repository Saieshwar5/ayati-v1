-- D246 (2026-11-07): the people in a conversation see agents talking. A handoff
-- asked from the main timeline is posted there with the helper's answer, so its
-- receipt has no thread (NULL). One asked in a thread stays in that thread.
-- Earlier receipts keep the thread under the sender's reply they were saved in.
ALTER TABLE relay_agent_handoffs ALTER COLUMN thread DROP NOT NULL;
