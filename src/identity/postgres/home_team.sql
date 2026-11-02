-- Schema 60: team panels (people-and-agents-first, Phase 3 HP4). A panel's owner can
-- show it to a channel's members or the whole workspace. Only panels whose every
-- value is read straight from the source can be shared, and viewers see only
-- snapshots whose values were checked against the source reads.
ALTER TABLE relay_home_panels
 ADD COLUMN audience TEXT NOT NULL DEFAULT 'personal' CHECK(audience IN ('personal','channel','workspace')),
 ADD COLUMN audience_chat TEXT,
 ADD COLUMN audience_revision BIGINT NOT NULL DEFAULT 0 CHECK(audience_revision>=0),
 ADD CONSTRAINT relay_home_audience_chat CHECK((audience='channel')=(audience_chat IS NOT NULL)),
 ADD CONSTRAINT relay_home_audience_chat_ref FOREIGN KEY(workspace,audience_chat) REFERENCES relay_chats(workspace,id);
CREATE INDEX relay_home_panels_team ON relay_home_panels(workspace,audience) WHERE audience<>'personal';
-- Every value of this update's snapshot was checked against its source reads.
ALTER TABLE relay_home_update_runs ADD COLUMN source_checked BOOLEAN NOT NULL DEFAULT false;

-- Who a team audience is.
CREATE FUNCTION relay_home_team_people(team_workspace text, team_kind text, team_chat text) RETURNS SETOF text LANGUAGE sql STABLE AS $$
 SELECT m.user_id FROM relay_members m WHERE m.workspace=team_workspace AND team_kind='workspace'
 UNION SELECT c.user_id FROM relay_chat_people c WHERE c.workspace=team_workspace AND team_kind='channel' AND c.chat=team_chat
$$;
CREATE FUNCTION relay_home_team_notify(w text, person text) RETURNS void LANGUAGE plpgsql AS $$
DECLARE n bigint;
BEGIN
 INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1) ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
 INSERT INTO relay_sync_events(workspace,cursor,kind,audience) VALUES(w,n,'panel_access',person);
 PERFORM pg_notify('relay_message_changes',w);
END $$;
-- A shared panel's new snapshot, or a changed audience (old and new), refreshes
-- each viewer's Home. No ids, titles or values on the wire.
CREATE FUNCTION relay_home_team_snapshot() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE person text;
BEGIN
 FOR person IN SELECT relay_home_team_people(p.workspace,p.audience,p.audience_chat) FROM relay_home_panels p
   WHERE p.workspace=NEW.workspace AND p.id=NEW.panel AND p.audience<>'personal'
 LOOP PERFORM relay_home_team_notify(NEW.workspace,person); END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_home_team_snapshot AFTER INSERT ON relay_home_snapshots
 FOR EACH ROW EXECUTE FUNCTION relay_home_team_snapshot();
CREATE FUNCTION relay_home_team_audience() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE person text;
BEGIN
 FOR person IN SELECT relay_home_team_people(NEW.workspace,OLD.audience,OLD.audience_chat)
   UNION SELECT relay_home_team_people(NEW.workspace,NEW.audience,NEW.audience_chat)
 LOOP PERFORM relay_home_team_notify(NEW.workspace,person); END LOOP;
 RETURN NEW;
END $$;
CREATE TRIGGER relay_home_team_audience AFTER UPDATE OF audience,audience_chat ON relay_home_panels
 FOR EACH ROW EXECUTE FUNCTION relay_home_team_audience();
