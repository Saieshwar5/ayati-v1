-- Schema 123: threads that work for people and agents.
--
-- Where agents answer in a channel: 'thread' puts an agent's answer to a
-- person's message in a thread under that message, so the channel stays
-- readable; 'channel' answers in the main channel, as before. Existing
-- channels and groups take the new default.
ALTER TABLE relay_chats ADD COLUMN agent_replies TEXT NOT NULL DEFAULT 'thread'
 CHECK(agent_replies IN ('thread','channel'));

-- A person follows a thread they started, wrote in, were mentioned in or asked
-- an agent in; replies there are "for them". A row records an explicit choice
-- (Stop following, or Follow), which wins over that.
CREATE TABLE relay_thread_follows (
 workspace TEXT NOT NULL, chat TEXT NOT NULL, thread TEXT NOT NULL, user_id TEXT NOT NULL,
 following BOOLEAN NOT NULL,
 at BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,chat,thread,user_id),
 FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);

CREATE FUNCTION relay_thread_followed(w TEXT, c TEXT, t TEXT, person TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT COALESCE(
  (SELECT f.following FROM relay_thread_follows f WHERE f.workspace=w AND f.chat=c AND f.thread=t AND f.user_id=person),
  EXISTS(SELECT 1 FROM relay_chat_messages m WHERE m.workspace=w AND m.chat=c AND (m.id=t OR m.thread=t)
   AND m.state<>'deleted'
   AND ((m.kind='user' AND m.author=person)
     OR strpos(m.content,'](mention:user:'||person||')')>0
     OR (m.kind='agent' AND m.requester=person))))
$$;
