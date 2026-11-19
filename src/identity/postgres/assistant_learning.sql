-- Schema 137: an assistant learns how its person writes from their edits.
--
-- When a member sends a message their assistant drafted after changing it, the
-- assistant is shown both texts once and may keep a line about how they write.
-- On unless the member turns it off on their assistant's page.
ALTER TABLE relay_assistant_settings ADD COLUMN learn BOOLEAN NOT NULL DEFAULT true;
