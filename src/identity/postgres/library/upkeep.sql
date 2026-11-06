-- Schema 77 (Library LB5/LB7): review dates, and questions agents couldn't answer from
-- the Library, kept apart from the items so they fire no item triggers.
CREATE TABLE relay_library_reviews (
 workspace TEXT NOT NULL, item TEXT NOT NULL,
 every_days INTEGER NOT NULL CHECK(every_days BETWEEN 7 AND 730),
 reviewed BIGINT NOT NULL, set_by TEXT NOT NULL,
 PRIMARY KEY(workspace,item),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
CREATE TABLE relay_library_gaps (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 query TEXT NOT NULL CHECK(char_length(query) BETWEEN 1 AND 256),
 agent TEXT NOT NULL, asked INTEGER NOT NULL DEFAULT 1, first_at BIGINT NOT NULL, last_at BIGINT NOT NULL,
 state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','answered','dismissed')),
 answer TEXT, PRIMARY KEY(workspace,id)
);
CREATE UNIQUE INDEX relay_library_gaps_open ON relay_library_gaps(workspace,agent,lower(query)) WHERE state='open';
