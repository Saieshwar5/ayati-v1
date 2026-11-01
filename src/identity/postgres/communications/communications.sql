-- External endpoints are explicitly simulated until a real verified adapter exists.
CREATE TABLE relay_communication_endpoints (
    id TEXT PRIMARY KEY, workspace TEXT NOT NULL REFERENCES relay_workspaces(id),
    agent TEXT NOT NULL, transport TEXT NOT NULL CHECK(transport IN ('email','whatsapp')),
    address TEXT NOT NULL CHECK(length(address) BETWEEN 1 AND 320),
    enabled BOOLEAN NOT NULL DEFAULT TRUE, revision BIGINT NOT NULL DEFAULT 1,
    simulated BOOLEAN NOT NULL DEFAULT TRUE CHECK(simulated),
    UNIQUE(workspace,id), UNIQUE(transport,address),
    FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id)
);
CREATE TABLE relay_communication_bindings (
    workspace TEXT NOT NULL, endpoint TEXT NOT NULL, sender TEXT NOT NULL,
    user_id TEXT NOT NULL REFERENCES relay_users(id),
    enabled BOOLEAN NOT NULL DEFAULT TRUE, revision BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY(workspace,endpoint,sender),
    FOREIGN KEY(workspace,endpoint) REFERENCES relay_communication_endpoints(workspace,id)
);
CREATE TABLE relay_communication_conversations (
    workspace TEXT NOT NULL, endpoint TEXT NOT NULL, sender TEXT NOT NULL,
    conversation TEXT NOT NULL, endpoint_revision BIGINT NOT NULL,
    binding_revision BIGINT NOT NULL, chat TEXT NOT NULL,
    PRIMARY KEY(workspace,endpoint,sender,conversation,endpoint_revision,binding_revision),
    UNIQUE(workspace,chat),
    FOREIGN KEY(workspace,endpoint,sender) REFERENCES relay_communication_bindings(workspace,endpoint,sender),
    FOREIGN KEY(workspace,chat) REFERENCES relay_chats(workspace,id)
);
CREATE TABLE relay_communication_inbox (
    workspace TEXT NOT NULL REFERENCES relay_workspaces(id), endpoint TEXT NOT NULL,
    event TEXT NOT NULL, transport TEXT NOT NULL CHECK(transport IN ('chat','email','whatsapp')),
    fingerprint TEXT NOT NULL, envelope JSONB NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('admitted','held','ignored')),
    message TEXT, reply TEXT, chat TEXT,
    sequence BIGINT GENERATED ALWAYS AS IDENTITY,
    PRIMARY KEY(workspace,endpoint,event), UNIQUE(workspace,message), UNIQUE(workspace,reply),
    FOREIGN KEY(workspace,message) REFERENCES relay_chat_messages(workspace,id),
    FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id)
);
CREATE TABLE relay_communication_outbox (
    workspace TEXT NOT NULL, id TEXT NOT NULL, endpoint TEXT NOT NULL, event TEXT NOT NULL,
    transport TEXT NOT NULL CHECK(transport IN ('chat','email','whatsapp')),
    envelope JSONB NOT NULL, job JSONB NOT NULL, source_claim TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('queued','sending','accepted','delivered','published','failed','uncertain','cancelled')),
    attempt TEXT, provider_id TEXT, lease_until BIGINT NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0, sequence BIGINT GENERATED ALWAYS AS IDENTITY,
    PRIMARY KEY(workspace,id),
    FOREIGN KEY(workspace,endpoint,event) REFERENCES relay_communication_inbox(workspace,endpoint,event)
);
CREATE INDEX relay_communication_inbox_order ON relay_communication_inbox(workspace,sequence);
CREATE INDEX relay_communication_outbox_order ON relay_communication_outbox(workspace,sequence);

CREATE FUNCTION relay_communication_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE mutable TEXT[];
BEGIN
 IF TG_OP='DELETE' THEN RAISE EXCEPTION 'Communication evidence cannot be deleted'; END IF;
 mutable:=CASE TG_TABLE_NAME
   WHEN 'relay_communication_outbox' THEN ARRAY['state','attempt','provider_id','lease_until','attempts']
   WHEN 'relay_communication_endpoints' THEN ARRAY['enabled','revision']
   WHEN 'relay_communication_bindings' THEN ARRAY['enabled','revision']
   ELSE ARRAY[]::TEXT[] END;
 IF (to_jsonb(NEW)-mutable) IS DISTINCT FROM (to_jsonb(OLD)-mutable) THEN
   RAISE EXCEPTION 'Communication identity and envelope are immutable';
 END IF;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_communication_inbox_immutable BEFORE UPDATE OR DELETE ON relay_communication_inbox
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();
CREATE TRIGGER relay_communication_outbox_immutable BEFORE UPDATE OR DELETE ON relay_communication_outbox
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();
CREATE TRIGGER relay_communication_endpoint_immutable BEFORE UPDATE OR DELETE ON relay_communication_endpoints
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();
CREATE TRIGGER relay_communication_binding_immutable BEFORE UPDATE OR DELETE ON relay_communication_bindings
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();
CREATE TRIGGER relay_communication_conversation_immutable BEFORE UPDATE OR DELETE ON relay_communication_conversations
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();

-- An endpoint or identity generation change never revives old context or sends.
CREATE FUNCTION relay_communication_current(w TEXT, c TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM relay_communication_conversations WHERE workspace=w AND chat=c)
 OR EXISTS(
    SELECT 1 FROM relay_communication_conversations v
    JOIN relay_communication_endpoints e ON e.workspace=v.workspace AND e.id=v.endpoint
    JOIN relay_communication_bindings b ON b.workspace=v.workspace AND b.endpoint=v.endpoint AND b.sender=v.sender
    JOIN relay_members m ON m.workspace=v.workspace AND m.user_id=b.user_id
    JOIN relay_shared_agents a ON a.workspace=v.workspace AND a.id=e.agent
    JOIN relay_chats ch ON ch.workspace=v.workspace AND ch.id=v.chat
    WHERE v.workspace=w AND v.chat=c AND e.enabled AND b.enabled
      AND e.revision=v.endpoint_revision AND b.revision=v.binding_revision
      AND ch.visibility='private' AND NOT ch.archived
      AND NOT COALESCE((a.settings->>'paused')::BOOLEAN,FALSE)
      AND (a.scope<>'private' OR a.owner_id=b.user_id)
      AND (m.role='Owner' OR a.owner_id=b.user_id OR EXISTS(
          SELECT 1 FROM relay_chat_access x WHERE x.workspace=w AND x.user_id=b.user_id AND x.agent=e.agent))
      AND (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=w AND p.chat=c)=1
      AND EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=w AND p.chat=c AND p.user_id=b.user_id)
      AND (SELECT count(*) FROM relay_chat_agents x WHERE x.workspace=w AND x.chat=c)=1
      AND EXISTS(SELECT 1 FROM relay_chat_agents x WHERE x.workspace=w AND x.chat=c AND x.agent=e.agent)
 )
$$;
