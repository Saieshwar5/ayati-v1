-- Schema 98: who has Cuckoo open, and the status people set (presence.rs, D245).
-- One row per member, updated in place: never a history (D226).
CREATE TABLE relay_member_presence (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 -- The last beat (Unix ms) while one of the person's live streams was open.
 seen_ms BIGINT CHECK(seen_ms>0),
 -- Set by a beat; cleared by the sweep once the last beat is too old.
 online BOOLEAN NOT NULL DEFAULT FALSE,
 -- "Show when I'm online" (Settings -> Preferences).
 visible BOOLEAN NOT NULL DEFAULT TRUE,
 status_kind TEXT CHECK(status_kind IN ('working','busy','meeting','away','custom')),
 status_emoji TEXT CHECK(octet_length(status_emoji) BETWEEN 1 AND 64),
 status_text TEXT CHECK(char_length(status_text) BETWEEN 1 AND 80),
 status_clear_ms BIGINT,
 status_set_ms BIGINT,
 -- Raised whenever what other members see changes; browsers keep the newest.
 revision BIGINT NOT NULL DEFAULT 0 CHECK(revision>=0),
 PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE,
 CHECK(NOT online OR seen_ms IS NOT NULL),
 CHECK((status_kind IS NULL)=(status_set_ms IS NULL)),
 CHECK(status_kind IS NOT NULL OR (status_emoji IS NULL AND status_text IS NULL AND status_clear_ms IS NULL)),
 CHECK(status_kind IS DISTINCT FROM 'custom' OR status_text IS NOT NULL)
);
-- The sweep's one deliberate look across workspaces, like the dispatcher's:
-- only the people still marked online.
CREATE INDEX relay_member_presence_online ON relay_member_presence(seen_ms) WHERE online;
