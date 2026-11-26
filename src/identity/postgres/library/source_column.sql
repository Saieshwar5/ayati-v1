-- Schema 160: each item keeps the item whose sharing it follows. Before, every list,
-- search and access check walked up the folder tree for every row. The walk now runs
-- only when an item is filed or a folder's own sharing is switched, for that item and
-- everything inside it; reading is one indexed lookup.
ALTER TABLE relay_library_items ADD COLUMN source TEXT;
ALTER FUNCTION relay_library_source(TEXT, TEXT) RENAME TO relay_library_source_walk;
UPDATE relay_library_items SET source=relay_library_source_walk(workspace,id);
CREATE INDEX relay_library_by_source ON relay_library_items(workspace,source);
CREATE FUNCTION relay_library_source(TEXT, TEXT) RETURNS TEXT LANGUAGE sql STABLE AS $$
 SELECT COALESCE(source,id) FROM relay_library_items WHERE workspace=$1 AND id=$2
$$;
CREATE FUNCTION relay_library_keep_source() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE relay_library_items i SET source=relay_library_source_walk(i.workspace,i.id)
 WHERE i.workspace=NEW.workspace
  AND (i.id=NEW.id OR i.id IN (SELECT relay_library_inside(NEW.workspace,NEW.id)))
  AND i.source IS DISTINCT FROM relay_library_source_walk(i.workspace,i.id);
 RETURN NULL;
END $$;
CREATE TRIGGER relay_library_keep_source AFTER INSERT OR UPDATE OF folder, shares
 ON relay_library_items FOR EACH ROW EXECUTE FUNCTION relay_library_keep_source();
