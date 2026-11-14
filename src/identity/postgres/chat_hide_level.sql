-- Schema 116: a conversation can be hidden from your Chat list until something
-- mentions you. Only your own level for one conversation; workspace defaults
-- (relay_workspace_settings.channel_level) keep all, mentions and none.
ALTER TABLE relay_chat_notify DROP CONSTRAINT relay_chat_notify_level_check;
ALTER TABLE relay_chat_notify ADD CONSTRAINT relay_chat_notify_level_check CHECK(level IN ('all','mentions','none','hide'));
