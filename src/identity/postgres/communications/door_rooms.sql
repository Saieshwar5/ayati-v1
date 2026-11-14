-- Schema 112 (E3): doors whose sender Cuckoo proves itself (WhatsApp, email)
-- deliver into the person's direct conversation with the agent, so the agent
-- remembers the person on every door. The room no longer carries the door's
-- generation; each request does. A request records the endpoint and binding
-- generations it was admitted under, and it stops (its unsent reply is
-- cancelled, its work loses access) when either changes. The room stays.
CREATE TABLE relay_communication_door_requests (
 workspace TEXT NOT NULL,
 reply TEXT NOT NULL,
 endpoint TEXT NOT NULL,
 sender TEXT NOT NULL,
 endpoint_revision BIGINT NOT NULL,
 binding_revision BIGINT NOT NULL,
 PRIMARY KEY(workspace,reply),
 FOREIGN KEY(workspace,endpoint,sender) REFERENCES relay_communication_bindings(workspace,endpoint,sender),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id)
);
CREATE TRIGGER relay_communication_door_request_immutable BEFORE UPDATE OR DELETE ON relay_communication_door_requests
 FOR EACH ROW EXECUTE FUNCTION relay_communication_immutable();

-- Whether request `r` may still run and send: true for requests that did not
-- come through a door; for a door request, its endpoint and binding are still
-- the generations it came under, and the person may still talk to the agent.
CREATE FUNCTION relay_communication_request_current(w TEXT, r TEXT) RETURNS BOOLEAN
LANGUAGE SQL STABLE AS $$
 SELECT NOT EXISTS(SELECT 1 FROM relay_communication_door_requests WHERE workspace=w AND reply=r)
 OR EXISTS(
    SELECT 1 FROM relay_communication_door_requests d
    JOIN relay_communication_endpoints e ON e.workspace=d.workspace AND e.id=d.endpoint
    JOIN relay_communication_bindings b ON b.workspace=d.workspace AND b.endpoint=d.endpoint AND b.sender=d.sender
    JOIN relay_shared_agents a ON a.workspace=d.workspace AND a.id=e.agent
    WHERE d.workspace=w AND d.reply=r AND e.enabled AND b.enabled
      AND e.revision=d.endpoint_revision AND b.revision=d.binding_revision
      AND NOT COALESCE((a.settings->>'paused')::BOOLEAN,FALSE)
      AND relay_agent_can_talk(w,e.agent,b.user_id)
 )
$$;
