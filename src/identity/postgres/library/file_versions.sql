-- Schema 152: files keep versions like pages and tables. A version of a file names the
-- stored object it was; replacing a file adds a version and restoring one makes the next.
-- The latest 20 are kept (every one while on legal hold); objects of versions dropped
-- wait in relay_library_discarded for the keeping sweep to remove them from storage.
ALTER TABLE relay_library_versions ALTER COLUMN document DROP NOT NULL;
ALTER TABLE relay_library_versions
 ADD COLUMN object_key TEXT, ADD COLUMN mime TEXT NOT NULL DEFAULT '',
 ADD COLUMN size BIGINT NOT NULL DEFAULT 0, ADD COLUMN checksum TEXT NOT NULL DEFAULT '',
 ADD COLUMN readiness TEXT NOT NULL DEFAULT 'ready';
INSERT INTO relay_library_versions(workspace,item,revision,author,title,text,created,object_key,mime,size,checksum,readiness)
 SELECT workspace,id,revision,created_by,title,text,updated,object_key,mime,size,checksum,readiness
 FROM relay_library_items WHERE kind='file' AND object_key IS NOT NULL
 ON CONFLICT DO NOTHING;
CREATE INDEX relay_library_version_objects ON relay_library_versions(workspace,object_key) WHERE object_key IS NOT NULL;
-- An upload that replaces a file: the item and the revision it was read at.
ALTER TABLE relay_library_uploads ADD COLUMN item TEXT, ADD COLUMN base BIGINT;
CREATE TABLE relay_library_discarded (
 workspace TEXT NOT NULL, object_key TEXT NOT NULL, at BIGINT NOT NULL,
 PRIMARY KEY(workspace,object_key)
);
