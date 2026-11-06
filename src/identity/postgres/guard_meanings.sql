-- Schema 65: what each action does (its meaning), safe defaults from it, company rules
-- for every action of one meaning, and time-of-day rules (core-agent CA1, part 2).
--
-- Meanings: read, change, money, sends_outside, delete. A reviewed action's effect
-- (read/write) stays the provider-facing fact; the meaning is Cuckoo's own review.
-- Cuckoo suggests one from the action's name; an admin may confirm or change it.
CREATE FUNCTION relay_guess_meaning(op JSONB) RETURNS TEXT LANGUAGE SQL IMMUTABLE AS $$
 SELECT CASE
  WHEN op->>'effect'='read' THEN 'read'
  WHEN concat_ws(' ',op->>'id',op->>'tool',op->>'title') ~* '(delete|remove|destroy|erase|purge|archive)' THEN 'delete'
  WHEN concat_ws(' ',op->>'id',op->>'tool',op->>'title') ~* '(refund|payment|pay[_ .-]|^pay|charge|invoice|transfer|payout|credit|debit)' THEN 'money'
  WHEN concat_ws(' ',op->>'id',op->>'tool',op->>'title') ~* '(send|e-?mail|message|reply|post|sms|whatsapp|notify|publish)' THEN 'sends_outside'
  ELSE 'change' END $$;
CREATE TABLE relay_action_meanings (
 workspace TEXT NOT NULL, connection TEXT NOT NULL, operation TEXT NOT NULL,
 meaning TEXT NOT NULL CHECK(meaning IN ('read','change','money','sends_outside','delete')),
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0),
 updated_by TEXT NOT NULL REFERENCES relay_users(id),
 updated_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,connection,operation),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id) ON DELETE CASCADE
);
CREATE FUNCTION relay_action_meaning(w TEXT, c TEXT, op JSONB) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT COALESCE((SELECT meaning FROM relay_action_meanings m WHERE m.workspace=w AND m.connection=c AND m.operation=op->>'id'),relay_guess_meaning(op)) $$;
-- The workspace rule when an admin set none: reading is allowed, deleting is blocked,
-- everything else asks first.
CREATE FUNCTION relay_default_mode(w TEXT, c TEXT, op JSONB) RETURNS TEXT LANGUAGE SQL STABLE AS $$
 SELECT CASE relay_action_meaning(w,c,op) WHEN 'read' THEN 'allow' WHEN 'delete' THEN 'block' ELSE 'ask' END $$;

DROP VIEW relay_control_effective_rules;
CREATE VIEW relay_control_effective_rules AS
 SELECT x.workspace,x.agent,x.connection,x.operation,x.mode,x.needs_other,x.revision,x.approver FROM (
  SELECT ac.workspace,ac.agent,ac.connection,op->>'id' AS operation,
   relay_stricter(COALESCE(w.mode,relay_default_mode(c.workspace,c.id,op)),COALESCE(r.mode,'allow')) AS mode,
   COALESCE(w.needs_other,false) AS needs_other,
   GREATEST(ac.revision,COALESCE(w.revision,0),COALESCE(r.changed,0),COALESCE(m.revision,0)) AS revision,a.owner_id AS approver
  FROM relay_agent_connections ac
  JOIN relay_shared_agents a ON a.workspace=ac.workspace AND a.id=ac.agent
  JOIN relay_control_connections c ON c.workspace=ac.workspace AND c.id=ac.connection
  JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server
  CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') op
  LEFT JOIN relay_connector_rules w ON w.workspace=ac.workspace AND w.connection=ac.connection AND w.operation=op->>'id'
  LEFT JOIN relay_control_rules r ON r.workspace=ac.workspace AND r.agent=ac.agent AND r.connection=ac.connection AND r.operation=op->>'id'
  LEFT JOIN relay_action_meanings m ON m.workspace=ac.workspace AND m.connection=ac.connection AND m.operation=op->>'id'
  WHERE s.approved AND c.enabled AND a.owner_id IS NOT NULL
 ) x WHERE x.mode<>'block';

-- Requests remember their meaning, so totals add up across every money action.
ALTER TABLE relay_control_actions ADD COLUMN meaning TEXT
 CHECK(meaning IS NULL OR meaning IN ('read','change','money','sends_outside','delete'));

-- Rules for one action, or for every action of one meaning; and time-of-day rules
-- (value {"from":"21:00","to":"09:00","days":[6,7]}, days ISO 1=Monday..7=Sunday).
ALTER TABLE relay_guard_rules ALTER COLUMN operation DROP NOT NULL,
 ADD COLUMN meaning TEXT CHECK(meaning IS NULL OR meaning IN ('read','change','money','sends_outside','delete'));
DO $$ DECLARE c RECORD; BEGIN
 FOR c IN SELECT conname FROM pg_constraint WHERE conrelid='relay_guard_rules'::regclass AND contype='c' LOOP
  EXECUTE format('ALTER TABLE relay_guard_rules DROP CONSTRAINT %I', c.conname);
 END LOOP; END $$;
ALTER TABLE relay_guard_rules
 ADD CONSTRAINT relay_guard_rules_target CHECK((operation IS NULL) <> (meaning IS NULL) AND (connection IS NULL OR operation IS NOT NULL)),
 ADD CONSTRAINT relay_guard_rules_operation_size CHECK(operation IS NULL OR char_length(operation) BETWEEN 1 AND 100),
 ADD CONSTRAINT relay_guard_rules_kind CHECK(kind IN ('field','daily_total','time')),
 ADD CONSTRAINT relay_guard_rules_field_size CHECK(char_length(field) BETWEEN 1 AND 80 OR (kind='time' AND field='')),
 ADD CONSTRAINT relay_guard_rules_comparator CHECK(comparator IN ('>','>=','<','<=','=','!=')),
 ADD CONSTRAINT relay_guard_rules_value_type CHECK(jsonb_typeof(value) IN ('number','string','boolean') OR (kind='time' AND jsonb_typeof(value)='object')),
 ADD CONSTRAINT relay_guard_rules_effect CHECK(effect IN ('ask','block')),
 ADD CONSTRAINT relay_guard_rules_note_size CHECK(char_length(note)<=280),
 ADD CONSTRAINT relay_guard_rules_revision CHECK(revision>0),
 ADD CONSTRAINT relay_guard_rules_total CHECK(kind<>'daily_total' OR (comparator IN ('>','>=') AND jsonb_typeof(value)='number')),
 ADD CONSTRAINT relay_guard_rules_order CHECK(kind<>'field' OR comparator IN ('=','!=') OR jsonb_typeof(value)='number');
CREATE INDEX relay_guard_rules_meaning ON relay_guard_rules(workspace,meaning) WHERE enabled AND meaning IS NOT NULL;
