-- A person's job title, short introduction and photo within one workspace.
-- Everyone in the workspace can read them; only the person can change them.
CREATE TABLE relay_member_profiles (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 title TEXT NOT NULL DEFAULT '' CHECK(char_length(title)<=80),
 about TEXT NOT NULL DEFAULT '' CHECK(char_length(about)<=200),
 photo TEXT, photo_mime TEXT CHECK(photo_mime IN ('image/png','image/jpeg','image/webp')),
 photo_bytes BYTEA CHECK(octet_length(photo_bytes) BETWEEN 1 AND 2097152),
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0),
 PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE,
 CHECK((photo IS NULL)=(photo_bytes IS NULL) AND (photo IS NULL)=(photo_mime IS NULL))
);
-- Other members' open chat screens reload the directory when a profile changes.
CREATE TRIGGER relay_member_profile_change AFTER INSERT OR UPDATE ON relay_member_profiles
 FOR EACH ROW EXECUTE FUNCTION relay_record_change();

-- Adding or removing an agent from a channel updates members' open screens.
CREATE TRIGGER relay_chat_agents_change AFTER INSERT OR DELETE ON relay_chat_agents
 FOR EACH ROW EXECUTE FUNCTION relay_record_change();
