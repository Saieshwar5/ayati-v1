-- The asking message of request `j` with reply `m`: what Activity shows as its
-- summary, never the assembled model context. New requests record it
-- (interactive_reserve.sql); older ones find the person's message just before the
-- reply. Used as a subquery so it runs only for rows that need it.
SELECT r.id,r.content FROM relay_chat_messages r WHERE r.id=j.request_message
UNION ALL
(SELECT r.id,r.content FROM relay_chat_messages r
 WHERE j.request_message IS NULL AND r.workspace=m.workspace AND r.chat=m.chat AND r.sequence<m.sequence
 AND r.kind='user' AND r.author=j.payload->>'requester' AND r.target=j.payload->>'agent'
 AND r.thread IS NOT DISTINCT FROM m.thread
 ORDER BY r.sequence DESC LIMIT 1)
