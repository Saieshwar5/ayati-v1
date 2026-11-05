-- Schema 64: the Guard's company rules (core-agent CA1). A rule adds one condition to
-- one reviewed action. It can only make the action stricter: ask first, or block.
-- kind 'field': compares one field of the exact request with a value.
-- kind 'daily_total': adds this request's field to the same action's total since the
-- start of the workspace's day; pending approvals count, so they reserve the amount.
CREATE TABLE relay_guard_rules (
 workspace TEXT NOT NULL REFERENCES relay_workspaces(id), id TEXT NOT NULL,
 operation TEXT NOT NULL CHECK(char_length(operation) BETWEEN 1 AND 100),
 -- NULL: every connected account that offers this action.
 connection TEXT,
 kind TEXT NOT NULL CHECK(kind IN ('field','daily_total')),
 field TEXT NOT NULL CHECK(char_length(field) BETWEEN 1 AND 80),
 comparator TEXT NOT NULL CHECK(comparator IN ('>','>=','<','<=','=','!=')),
 value JSONB NOT NULL CHECK(jsonb_typeof(value) IN ('number','string','boolean')),
 effect TEXT NOT NULL CHECK(effect IN ('ask','block')),
 note TEXT NOT NULL DEFAULT '' CHECK(char_length(note)<=280),
 enabled BOOLEAN NOT NULL DEFAULT true,
 revision BIGINT NOT NULL DEFAULT 1 CHECK(revision>0),
 created_by TEXT NOT NULL REFERENCES relay_users(id),
 updated_by TEXT NOT NULL REFERENCES relay_users(id),
 created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 updated_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,id),
 CHECK(kind='field' OR (comparator IN ('>','>=') AND jsonb_typeof(value)='number')),
 CHECK(comparator IN ('=','!=') OR jsonb_typeof(value)='number'),
 FOREIGN KEY(workspace,connection) REFERENCES relay_control_connections(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_guard_rules_operation ON relay_guard_rules(workspace,operation) WHERE enabled;
-- The decision record: which rule held the request and the facts it used.
ALTER TABLE relay_control_actions ADD COLUMN guard JSONB;
