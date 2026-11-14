-- Device sessions (schema 117, runtime/device-sessions.md): a phone signs in by
-- scanning a one-time code that a signed-in web session made. Codes and device
-- tokens are stored only as SHA-256 hashes; each is shown once.

-- One sign-in code per person: a new code replaces the previous one. The window
-- columns count codes made in the current hour (creation is capped).
CREATE TABLE relay_device_link_codes (
 user_id TEXT PRIMARY KEY REFERENCES relay_users(id) ON DELETE CASCADE,
 code_hash TEXT NOT NULL UNIQUE CHECK(length(code_hash)=43),
 created BIGINT NOT NULL,
 expires BIGINT NOT NULL,
 used BIGINT,
 window_start BIGINT NOT NULL,
 window_count INTEGER NOT NULL
);

-- A signed-in phone. `expires` is absolute (no sliding renewal); `revoked` is
-- set when the person signs the device out. `last_used` is refreshed at most
-- once a minute.
CREATE TABLE relay_device_sessions (
 id TEXT PRIMARY KEY,
 user_id TEXT NOT NULL REFERENCES relay_users(id) ON DELETE CASCADE,
 token_hash TEXT NOT NULL UNIQUE CHECK(length(token_hash)=43),
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 60),
 platform TEXT NOT NULL CHECK(platform IN ('ios','android')),
 created BIGINT NOT NULL,
 last_used BIGINT NOT NULL,
 expires BIGINT NOT NULL,
 revoked BIGINT
);
CREATE INDEX relay_device_sessions_by_user ON relay_device_sessions(user_id, created);
