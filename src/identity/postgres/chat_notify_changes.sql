-- Schema 140: a person's notification choices reach their other devices at once.
-- Changing a conversation's bell (relay_chat_notify) or the defaults in Settings
-- (relay_member_preferences) records a 'levels' change for that person only. It
-- names no conversation or level: their open pages read their levels again.
CREATE FUNCTION relay_notify_levels_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE n bigint;
BEGIN
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(NEW.workspace,1)
 ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events(workspace,cursor,kind,audience) VALUES(NEW.workspace,n,'levels',NEW.user_id);
 PERFORM pg_notify('relay_message_changes',NEW.workspace);
 RETURN NEW;
END $$;
CREATE TRIGGER relay_chat_notify_changed AFTER INSERT OR UPDATE ON relay_chat_notify
 FOR EACH ROW EXECUTE FUNCTION relay_notify_levels_changed();
CREATE TRIGGER relay_member_preferences_changed AFTER INSERT OR UPDATE ON relay_member_preferences
 FOR EACH ROW EXECUTE FUNCTION relay_notify_levels_changed();
