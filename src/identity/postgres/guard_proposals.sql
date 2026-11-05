-- Schema 67: creating company rules safely (core-agent CA2). Agents can propose a rule
-- from a person's words; it stays off until an admin reviews it. A rule can start with a
-- trial: until trial_until_ms it only records what it would have done.
ALTER TABLE relay_guard_rules
 ADD COLUMN proposed_by TEXT,
 ADD COLUMN proposal TEXT CHECK(proposal IS NULL OR char_length(proposal) BETWEEN 1 AND 280),
 ADD COLUMN reviewed BOOLEAN NOT NULL DEFAULT true,
 ADD COLUMN trial_until_ms BIGINT,
 ADD CONSTRAINT relay_guard_rules_proposal CHECK(reviewed OR (proposed_by IS NOT NULL AND NOT enabled)),
 ADD CONSTRAINT relay_guard_rules_proposer FOREIGN KEY(workspace,proposed_by) REFERENCES relay_shared_agents(workspace,id) ON DELETE SET NULL (proposed_by);
CREATE INDEX relay_guard_rules_proposed ON relay_guard_rules(workspace) WHERE NOT reviewed;
-- Trial matches live in the decision record: guard->'trial' lists rule ids.
CREATE INDEX relay_control_actions_guard_trial ON relay_control_actions USING gin((guard->'trial')) WHERE guard ? 'trial';
-- Requests in the same second keep their order for replay's running day totals.
ALTER TABLE relay_control_actions ADD COLUMN created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint;
