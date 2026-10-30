-- Invite-only early access: creating a workspace needs a single-use code that a
-- platform administrator issued. Codes are platform-level (no workspace yet) and
-- stored only as SHA-256 hashes; the code itself is shown once at creation.
CREATE TABLE relay_workspace_codes (
 id TEXT PRIMARY KEY,
 code_hash TEXT NOT NULL UNIQUE,
 note TEXT NOT NULL,
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 created_at BIGINT NOT NULL,
 expires_at BIGINT NOT NULL,
 revoked_at BIGINT,
 used_by TEXT REFERENCES relay_users(id),
 used_at BIGINT,
 workspace TEXT REFERENCES relay_workspaces(id) ON DELETE SET NULL
);
-- Failed code attempts per person, for a short window only (guessing defence).
CREATE TABLE relay_workspace_code_attempts (
 user_id TEXT NOT NULL REFERENCES relay_users(id) ON DELETE CASCADE,
 at BIGINT NOT NULL
);
CREATE INDEX relay_workspace_code_attempts_by_user ON relay_workspace_code_attempts(user_id, at);
