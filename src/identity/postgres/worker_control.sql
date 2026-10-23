-- Local machine credentials carry no customer authority without current assignment checks.
CREATE TABLE relay_executors (
 id TEXT PRIMARY KEY,
 incarnation TEXT NOT NULL,
 token_hash TEXT NOT NULL UNIQUE CHECK(length(token_hash)=43),
 expires BIGINT NOT NULL,
 UNIQUE(id,incarnation)
);
CREATE TABLE relay_assignment_credentials (
 assignment TEXT PRIMARY KEY REFERENCES relay_worker_assignments(id),
 executor TEXT NOT NULL,
 incarnation TEXT NOT NULL,
 slot TEXT NOT NULL UNIQUE,
 request TEXT NOT NULL,
 token_hash TEXT NOT NULL UNIQUE CHECK(length(token_hash)=43),
 expires BIGINT NOT NULL,
 session TEXT,
 context JSONB,
 completion_hash TEXT,
 completion_state TEXT CHECK(completion_state IN ('complete','failed','cancelled')),
 UNIQUE(executor,incarnation,request),
 FOREIGN KEY(executor,incarnation) REFERENCES relay_executors(id,incarnation)
);
