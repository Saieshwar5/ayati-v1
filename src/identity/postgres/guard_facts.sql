-- Schema 66: facts the agent didn't send (core-agent CA1, part 3). A rule can take its
-- value from a reading action the agent completed earlier in the same task, through
-- Cuckoo: {"operation":"order.read","match":"order","path":"total"} reads the completed
-- `order.read` whose `order` equals this request's `order`, and uses its `total`.
-- kind 'read_first': the action needs that read first ("read before you change").
ALTER TABLE relay_guard_rules ADD COLUMN source JSONB
 CHECK(source IS NULL OR (jsonb_typeof(source)='object' AND source ?& ARRAY['operation','match']));
ALTER TABLE relay_guard_rules DROP CONSTRAINT relay_guard_rules_kind,
 ADD CONSTRAINT relay_guard_rules_kind CHECK(kind IN ('field','daily_total','time','read_first')),
 ADD CONSTRAINT relay_guard_rules_read_first CHECK(kind<>'read_first' OR (source IS NOT NULL AND operation IS NOT NULL)),
 ADD CONSTRAINT relay_guard_rules_source_target CHECK(source IS NULL OR operation IS NOT NULL);
ALTER TABLE relay_guard_rules DROP CONSTRAINT relay_guard_rules_field_size,
 ADD CONSTRAINT relay_guard_rules_field_size CHECK(char_length(field) BETWEEN 1 AND 80 OR (kind IN ('time','read_first') AND field=''));
CREATE INDEX relay_control_actions_reads ON relay_control_actions(workspace,job,operation) WHERE state='completed';
