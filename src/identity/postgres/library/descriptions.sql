-- Schema 158: one line saying what an item is, written by the person or agent who saves
-- it, in the words people would look for. It is searched with the item's text, so a
-- file is found by what it is about without any model reading it.
ALTER TABLE relay_library_items ADD COLUMN description TEXT NOT NULL DEFAULT ''
 CHECK(char_length(description)<=300);
