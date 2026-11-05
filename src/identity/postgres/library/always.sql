-- Schema 73 (Library LB2c): documents an agent always has in every request, chosen by
-- its managers among documents it was given (at most 5).
CREATE TABLE relay_library_always (
 workspace TEXT NOT NULL, agent TEXT NOT NULL, item TEXT NOT NULL,
 position SMALLINT NOT NULL CHECK(position BETWEEN 0 AND 4),
 PRIMARY KEY(workspace,agent,item), UNIQUE(workspace,agent,position),
 FOREIGN KEY(workspace,agent) REFERENCES relay_shared_agents(workspace,id) ON DELETE CASCADE,
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
