CREATE TABLE memory_sources (
 id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id),
 kind TEXT NOT NULL CHECK(kind IN ('owner_note','owner_message')), text TEXT NOT NULL,
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE TABLE memories (
 id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id),
 key TEXT NOT NULL, content TEXT NOT NULL, source_id TEXT NOT NULL REFERENCES memory_sources(id),
 quote TEXT NOT NULL, supersedes TEXT REFERENCES memories(id),
 state TEXT NOT NULL CHECK(state IN ('active','superseded','forgotten')),
 created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE UNIQUE INDEX current_memory_key ON memories(conversation_id,key) WHERE state='active';
CREATE TABLE memory_exclusions (
 conversation_id TEXT NOT NULL REFERENCES conversations(id), key TEXT NOT NULL,
 PRIMARY KEY(conversation_id,key)
);
CREATE TABLE memory_source_exclusions (
 source_id TEXT PRIMARY KEY REFERENCES memory_sources(id)
);
CREATE VIRTUAL TABLE memory_fts USING fts5(content, tokenize='unicode61');
CREATE TRIGGER index_memory_insert AFTER INSERT ON memories WHEN NEW.state='active' BEGIN
 INSERT INTO memory_fts(rowid,content) VALUES(NEW.rowid,NEW.key || ' ' || NEW.content);
END;
CREATE TRIGGER index_memory_update AFTER UPDATE OF state ON memories WHEN OLD.state='active' BEGIN
 DELETE FROM memory_fts WHERE rowid=OLD.rowid;
END;
PRAGMA user_version=5;
