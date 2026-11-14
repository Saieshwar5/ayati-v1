-- Push notifications to signed-in phones (schema 116, mobile plan M4).
-- A registration belongs to one device session, so signing out or revoking
-- the phone removes it with the session.
CREATE TABLE relay_push_devices (
    device TEXT PRIMARY KEY REFERENCES relay_device_sessions(id) ON DELETE CASCADE,
    expo_token TEXT NOT NULL CHECK (length(expo_token) BETWEEN 10 AND 200),
    -- Kept so sending can move straight to APNs and FCM without an app change.
    native_token TEXT CHECK (native_token IS NULL OR length(native_token) BETWEEN 10 AND 4096),
    updated BIGINT NOT NULL
);
-- A token moves with the phone to its newest session.
CREATE UNIQUE INDEX relay_push_devices_token ON relay_push_devices(expo_token);

-- What has been pushed to whom. A row is claimed before sending, so each item
-- goes out at most once, whichever controller sends it.
CREATE TABLE relay_push_sent (
    workspace TEXT NOT NULL,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('message','approval','question')),
    ref TEXT NOT NULL,
    at BIGINT NOT NULL,
    PRIMARY KEY (workspace, user_id, kind, ref)
);
CREATE INDEX relay_push_sent_at ON relay_push_sent(at);

-- New messages are found by time, not by scanning every conversation.
CREATE INDEX relay_messages_by_time ON relay_chat_messages(workspace, at);
