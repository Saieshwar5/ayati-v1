-- Schema 154: the agent's workbench. The Library keeps; the agent's computer works. An
-- agent opens an item as a file on its computer, and each opened file remembers the item
-- and revision it came from, so saving it back makes that item's next version.
CREATE TABLE relay_library_opened (
 workspace TEXT NOT NULL, reply TEXT NOT NULL, path TEXT NOT NULL, item TEXT NOT NULL,
 revision BIGINT NOT NULL, at BIGINT NOT NULL, agent TEXT NOT NULL,
 PRIMARY KEY(workspace,reply,path),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_library_opened_item ON relay_library_opened(workspace,item);
-- Copies are removed from the computer when another request of the agent takes it.
CREATE INDEX relay_library_opened_agent ON relay_library_opened(workspace,agent);
