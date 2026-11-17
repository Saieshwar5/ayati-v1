-- Schema 129: what an assistant knows about its person, and its regular checks.
CREATE TABLE relay_assistant_settings (
 workspace TEXT NOT NULL, user_id TEXT NOT NULL,
 -- What the assistant keeps about its person; they can read and edit both.
 about TEXT NOT NULL DEFAULT '' CHECK(length(about)<=6000),
 style TEXT NOT NULL DEFAULT '' CHECK(length(style)<=3000),
 -- Regular checks: on or off, how often, and the person's own checklist.
 checks BOOLEAN NOT NULL DEFAULT false,
 every_minutes INTEGER NOT NULL DEFAULT 240 CHECK(every_minutes BETWEEN 30 AND 1440),
 checklist TEXT NOT NULL DEFAULT '' CHECK(length(checklist)<=3000),
 next_ms BIGINT, job TEXT,
 -- The newest message the last check looked at.
 checked BIGINT NOT NULL DEFAULT 0,
 updated_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
 PRIMARY KEY(workspace,user_id),
 FOREIGN KEY(workspace,user_id) REFERENCES relay_members(workspace,user_id) ON DELETE CASCADE
);
CREATE INDEX relay_assistant_checks_due ON relay_assistant_settings(next_ms) WHERE checks;
CREATE TRIGGER relay_assistant_checks_announced AFTER INSERT OR UPDATE OF checks,next_ms ON relay_assistant_settings
 FOR EACH STATEMENT EXECUTE FUNCTION relay_announce_work('schedule');
