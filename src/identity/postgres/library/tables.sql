-- Schema 76 (Library LB4): tables, structured lists with a key column (price lists,
-- product catalogues, supplier lists) kept as Library items with versions.
ALTER TABLE relay_library_items DROP CONSTRAINT relay_library_items_kind_check;
ALTER TABLE relay_library_items ADD CONSTRAINT relay_library_items_kind_check
 CHECK(kind IN ('page','file','folder','table'));
-- A company rule's limit can come from a table row: {table, match, column, title}.
ALTER TABLE relay_guard_rules DROP CONSTRAINT relay_guard_rules_order;
ALTER TABLE relay_guard_rules ADD CONSTRAINT relay_guard_rules_order
 CHECK(kind<>'field' OR comparator IN ('=','!=') OR jsonb_typeof(value)='number'
  OR (jsonb_typeof(value)='object' AND value ? 'table'));
ALTER TABLE relay_guard_rules DROP CONSTRAINT relay_guard_rules_value_type;
ALTER TABLE relay_guard_rules ADD CONSTRAINT relay_guard_rules_value_type
 CHECK(jsonb_typeof(value) IN ('number','string','boolean')
  OR (kind='time' AND jsonb_typeof(value)='object')
  OR (kind='field' AND jsonb_typeof(value)='object' AND value ? 'table'));
