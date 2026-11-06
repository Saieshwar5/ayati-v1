-- Schema 82 (LB7): keeping rules and legal holds. An admin sets how long a folder keeps
-- what is added to it; past that, Cuckoo deletes the item for good: its text, versions,
-- passages and stored file go, and a trashed tombstone stays so answers that cited it
-- stay hidden. A legal hold on an item or folder stops trashing, deleting and keeping
-- rules, and keeps every version, until an admin lifts it.
CREATE TABLE relay_library_keeping (
 workspace TEXT NOT NULL, folder TEXT NOT NULL,
 keep_days INTEGER NOT NULL CHECK(keep_days BETWEEN 1 AND 3650),
 set_by TEXT NOT NULL REFERENCES relay_users(id), set_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,folder),
 FOREIGN KEY(workspace,folder) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
CREATE TABLE relay_library_holds (
 workspace TEXT NOT NULL, item TEXT NOT NULL,
 reason TEXT NOT NULL CHECK(char_length(reason) BETWEEN 1 AND 300),
 held_by TEXT NOT NULL REFERENCES relay_users(id), held_at BIGINT NOT NULL,
 PRIMARY KEY(workspace,item),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
ALTER TABLE relay_library_items ADD COLUMN purged BIGINT;
