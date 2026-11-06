-- Schema 74 (Library LB3): agents' suggested edits waiting for a person, and which
-- agent and task wrote a page version.
CREATE TABLE relay_library_suggestions (
 workspace TEXT NOT NULL, id TEXT NOT NULL, item TEXT NOT NULL,
 base_revision BIGINT NOT NULL, title TEXT NOT NULL, document JSONB NOT NULL, text TEXT NOT NULL,
 note TEXT NOT NULL DEFAULT '' CHECK(char_length(note)<=500),
 agent TEXT NOT NULL, job TEXT NOT NULL, requester TEXT NOT NULL,
 state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','rejected')),
 created BIGINT NOT NULL, decided_by TEXT, decided BIGINT,
 PRIMARY KEY(workspace,id),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_library_suggestions_waiting ON relay_library_suggestions(workspace,item) WHERE state='pending';
ALTER TABLE relay_library_versions ADD COLUMN agent TEXT, ADD COLUMN job TEXT;
