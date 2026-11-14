-- Device sessions for the phone app (schema 115). A phone can't rely on the
-- browser's HttpOnly cookie, so a sign-in started by the app ends with a
-- one-time code the app redeems, with its PKCE verifier, for a bearer token.
-- Only digests of codes and tokens are stored.

-- The PKCE challenge of a sign-in the phone app started; NULL for the web.
ALTER TABLE relay_login_flows ADD COLUMN mobile_challenge TEXT
    CHECK (mobile_challenge IS NULL OR length(mobile_challenge)=43);

-- Codes live 60 seconds and are deleted when redeemed, right or wrong.
CREATE TABLE relay_mobile_codes (
    hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES relay_users(id),
    provider_session TEXT NOT NULL,
    challenge TEXT NOT NULL CHECK (length(challenge)=43),
    expires BIGINT NOT NULL
);
CREATE INDEX relay_mobile_codes_expiry ON relay_mobile_codes(expires);

CREATE TABLE relay_device_sessions (
    id TEXT PRIMARY KEY,
    hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL REFERENCES relay_users(id),
    provider_session TEXT NOT NULL,
    device_name TEXT NOT NULL CHECK (length(device_name) BETWEEN 1 AND 80),
    platform TEXT NOT NULL CHECK (platform IN ('ios','android')),
    created BIGINT NOT NULL,
    last_used BIGINT NOT NULL,
    expires BIGINT NOT NULL
);
CREATE INDEX relay_device_sessions_by_user ON relay_device_sessions(user_id, created);
CREATE INDEX relay_device_sessions_expiry ON relay_device_sessions(expires);
