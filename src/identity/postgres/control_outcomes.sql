-- Schema 107: unconfirmed writes are settled, by a person or by Cuckoo asking the
-- provider (runtime/connector-providers.md, "Unconfirmed writes").
ALTER TABLE relay_control_actions DROP CONSTRAINT relay_control_actions_state_check;
ALTER TABLE relay_control_actions ADD CONSTRAINT relay_control_actions_state_check
 CHECK(state IN ('pending','executing','completed','denied','rejected','invalidated','uncertain','not_sent'));
-- settled_as: 'happened' or 'not_happened' by a person (settled_by), 'checked' when
-- the provider found it happened (no person). A settled action is completed or not_sent.
ALTER TABLE relay_control_actions
 ADD COLUMN settled_by TEXT REFERENCES relay_users(id),
 ADD COLUMN settled_at BIGINT,
 ADD COLUMN settled_as TEXT CHECK(settled_as IN ('happened','not_happened','checked')),
 ADD COLUMN settled_note TEXT CHECK(char_length(settled_note)<=500),
 ADD CONSTRAINT relay_control_actions_settled CHECK(
  (settled_at IS NULL) = (settled_as IS NULL)
  AND (settled_as IS NULL OR (settled_as='checked') = (settled_by IS NULL))
  AND (state='not_sent') = (settled_as IS NOT DISTINCT FROM 'not_happened'));

-- Automatic checks, about 1, 5 and 30 minutes after an action became unconfirmed
-- (since). A controller leases a check before asking the provider, so several
-- controllers never ask twice at once. Kept apart from the action row, whose
-- updates wake waiting tasks and approval cards.
CREATE TABLE relay_control_outcome_checks (
 workspace TEXT NOT NULL, action TEXT NOT NULL,
 since BIGINT NOT NULL,
 attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 3),
 next_at BIGINT, lease_until BIGINT NOT NULL DEFAULT 0,
 -- [{at, result: unknown|happened|not_happened}], oldest first.
 checks JSONB NOT NULL DEFAULT '[]'::jsonb
  CHECK(jsonb_typeof(checks)='array' AND jsonb_array_length(checks)<=3),
 PRIMARY KEY(workspace,action),
 FOREIGN KEY(workspace,action) REFERENCES relay_control_actions(workspace,id)
);
CREATE INDEX relay_control_actions_unconfirmed ON relay_control_actions(updated)
 WHERE state IN ('executing','uncertain');
