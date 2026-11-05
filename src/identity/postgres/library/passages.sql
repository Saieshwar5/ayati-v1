-- Schema 70 (Library LB2a): documents cut into passages for full-text search by
-- section, the revision each item was last indexed at, and receipts of what agents
-- were shown by search (fenced like reads, but not cited).
CREATE TABLE relay_library_passages (
 workspace TEXT NOT NULL, item TEXT NOT NULL, position INTEGER NOT NULL,
 revision BIGINT NOT NULL, title TEXT NOT NULL, section TEXT NOT NULL DEFAULT '', text TEXT NOT NULL,
 search tsvector GENERATED ALWAYS AS (
  setweight(to_tsvector('english', title), 'A') ||
  setweight(to_tsvector('english', section), 'B') ||
  to_tsvector('english', text)) STORED,
 PRIMARY KEY(workspace,item,position),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_library_passage_search ON relay_library_passages USING GIN(search);
-- Kept apart from relay_library_items: indexing is bookkeeping, not an access change,
-- so it must not fire that table's context-fencing trigger.
CREATE TABLE relay_library_indexed (
 workspace TEXT NOT NULL, item TEXT NOT NULL, revision BIGINT NOT NULL,
 PRIMARY KEY(workspace,item),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
ALTER TABLE relay_library_reads
 ADD COLUMN cited BOOLEAN NOT NULL DEFAULT true,
 ADD COLUMN section TEXT NOT NULL DEFAULT '';
