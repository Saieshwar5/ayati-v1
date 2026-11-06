-- Schema 71: Library changes no longer fence the whole workspace's agent context.
-- Every way Library content reaches an agent now has its own receipt checked against
-- the item's access revision: Ask sources (relay_library_sources), agent reads and
-- search previews (relay_library_reads), rehearsal samples and Home panels (their own
-- triggers). A page edit therefore keeps earlier answers and running work; a sharing
-- change fences only the work that used that item.
DROP TRIGGER relay_context_library ON relay_library_items;
DROP TRIGGER relay_context_library_access ON relay_library_agents;
