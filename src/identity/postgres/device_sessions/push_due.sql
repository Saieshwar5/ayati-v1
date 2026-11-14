-- What phones should be told about now. $1 now (s), $2 the delay a message
-- waits so reading it on the web prevents a push, $3 how far back to look.
-- A message counts the way Chat counts unread (unread.rs), only from others,
-- never an agent's progress note, and only in active conversations the person
-- can see in a workspace that is open. Levels: the conversation's own, else
-- everything in direct messages and the person's default for channels.
WITH phones AS (
 SELECT DISTINCT s.user_id FROM relay_push_devices p
 JOIN relay_device_sessions s ON s.id=p.device WHERE s.expires>$1
), messages AS (
 SELECT m.workspace, m.chat, m.id AS ref, me.user_id, m.author, c.kind AS chat_kind,
  strpos(m.content,'](mention:user:'||me.user_id||')')>0 AS mentioned,
  (m.kind='agent' AND m.requester=me.user_id) AS replied,
  m.kind='agent' AS by_agent
 FROM phones ph
 JOIN relay_members me ON me.user_id=ph.user_id
 JOIN relay_workspaces w ON w.id=me.workspace
  AND to_jsonb(w)->>'closed_ms' IS NULL AND to_jsonb(w)->>'suspended_ms' IS NULL
 JOIN relay_chats c ON c.workspace=me.workspace AND NOT c.archived AND NOT c.work
 LEFT JOIN relay_chat_people p ON p.workspace=c.workspace AND p.chat=c.id AND p.user_id=me.user_id
 JOIN relay_chat_messages m ON m.workspace=c.workspace AND m.chat=c.id
 WHERE (c.visibility='public' OR p.user_id IS NOT NULL)
  AND m.author<>me.user_id AND m.state IN ('sent','complete') AND m.update_of IS NULL
  AND (m.at BETWEEN $1-$3 AND $1-$2 OR m.activity_at BETWEEN $1-$3 AND $1-$2)
  AND COALESCE(m.activity_at,m.at)<=$1-$2
  AND m.at>=greatest(me.joined_ms,CASE WHEN c.visibility='public' THEN 0 ELSE p.joined_ms END)/1000
  AND NOT EXISTS(SELECT 1 FROM relay_message_receipts r
   WHERE r.workspace=m.workspace AND r.message=m.id AND r.user_id=me.user_id AND r.seen_at IS NOT NULL)
), due AS (
 SELECT x.workspace, x.user_id, 'message' AS kind, x.ref, x.chat, x.author, x.by_agent,
  CASE WHEN x.replied THEN 'replied' WHEN x.mentioned THEN 'mentioned'
   WHEN x.chat_kind='direct' THEN 'direct' ELSE 'channel' END AS reason
 FROM messages x
 LEFT JOIN relay_chat_notify n ON n.workspace=x.workspace AND n.user_id=x.user_id AND n.chat=x.chat
 LEFT JOIN relay_member_preferences mp ON mp.workspace=x.workspace AND mp.user_id=x.user_id
 WHERE CASE COALESCE(n.level, CASE WHEN x.chat_kind='direct' THEN 'all' ELSE COALESCE(mp.channel_level,'mentions') END)
  WHEN 'all' THEN true WHEN 'mentions' THEN x.mentioned OR x.replied ELSE false END
 UNION ALL
 SELECT a.workspace, a.approver, 'approval', a.id, a.chat, a.agent, true, 'approval'
 FROM relay_control_actions a JOIN phones ph ON ph.user_id=a.approver
 LEFT JOIN relay_member_preferences mp ON mp.workspace=a.workspace AND mp.user_id=a.approver
 WHERE a.state='pending' AND a.requires_approval AND a.created BETWEEN $1-$3 AND $1
  AND COALESCE(mp.approval_alerts,true)
 UNION ALL
 SELECT q.workspace, j.payload->>'requester', 'question', q.id, m.chat, m.author, true, 'question'
 FROM relay_task_questions q
 JOIN relay_chat_jobs j ON j.workspace=q.workspace AND j.reply=q.reply
 JOIN relay_chat_messages m ON m.workspace=q.workspace AND m.id=q.reply
 JOIN phones ph ON ph.user_id=j.payload->>'requester'
 WHERE q.state='pending' AND j.state<>'finished' AND q.created_ms/1000 BETWEEN $1-$3 AND $1
)
SELECT d.workspace, d.user_id, d.kind, d.ref, d.chat, d.reason, w.name AS workspace_name,
 COALESCE((SELECT a.name FROM relay_shared_agents a WHERE a.workspace=d.workspace AND a.id=d.author),
  (SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=d.author), 'Cuckoo') AS author_name
FROM due d JOIN relay_workspaces w ON w.id=d.workspace
WHERE NOT EXISTS(SELECT 1 FROM relay_push_sent s
 WHERE s.workspace=d.workspace AND s.user_id=d.user_id AND s.kind=d.kind AND s.ref=d.ref)
LIMIT 200
