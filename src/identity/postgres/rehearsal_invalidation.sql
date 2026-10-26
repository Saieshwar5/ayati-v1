-- Personal invalidations reuse the durable live transport. Payloads contain no
-- sample/source IDs, titles, model output or identities for other recipients.
ALTER TABLE relay_sync_events ADD COLUMN audience TEXT;
CREATE INDEX relay_sample_library_refs ON relay_agent_rehearsals USING GIN((input->'sources'));
CREATE INDEX relay_sample_note_refs ON relay_agent_rehearsals USING GIN(note_refs);
CREATE FUNCTION relay_sample_source_changed() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE item jsonb; w text; resource text; person text; n bigint;
BEGIN
 item := CASE WHEN TG_OP='DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
 w := item->>'workspace';
 resource := CASE WHEN TG_TABLE_NAME='relay_library_agents' THEN item->>'item' ELSE item->>'id' END;
 UPDATE relay_agent_rehearsals r SET state='invalidated',claim=NULL,lease_until=0,prepared=NULL,output=NULL,
  detail='Selected knowledge changed. Start a new sample with current access.',
  finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE r.workspace=w AND r.state IN ('queued','running') AND (
  (TG_TABLE_NAME='relay_agent_knowledge' AND r.agent=item->>'agent'
   AND r.note_refs @> jsonb_build_array(jsonb_build_object('id',resource))) OR
  (TG_TABLE_NAME<>'relay_agent_knowledge'
   AND (TG_TABLE_NAME<>'relay_library_agents' OR r.agent=item->>'agent')
   AND (r.input->'sources') @> jsonb_build_array(jsonb_build_object('id',resource)))
 );
 FOR person IN
  SELECT DISTINCT r.actor FROM relay_agent_rehearsals r WHERE r.workspace=w AND (
   (TG_TABLE_NAME='relay_agent_knowledge' AND r.agent=item->>'agent'
    AND r.note_refs @> jsonb_build_array(jsonb_build_object('id',resource))) OR
   (TG_TABLE_NAME<>'relay_agent_knowledge'
    AND (TG_TABLE_NAME<>'relay_library_agents' OR r.agent=item->>'agent')
    AND (r.input->'sources') @> jsonb_build_array(jsonb_build_object('id',resource)))
  )
 LOOP
  INSERT INTO relay_sync_clocks(workspace,cursor) VALUES(w,1)
   ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor INTO n;
  INSERT INTO relay_sync_events(workspace,cursor,kind,audience) VALUES(w,n,'sample_access',person);
 END LOOP;
 IF n IS NOT NULL THEN PERFORM pg_notify('relay_message_changes',w); END IF;
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_sample_library_changed AFTER UPDATE OF revision,access_revision,trashed OR DELETE ON relay_library_items
 FOR EACH ROW EXECUTE FUNCTION relay_sample_source_changed();
CREATE TRIGGER relay_sample_library_grant_changed AFTER DELETE OR UPDATE ON relay_library_agents
 FOR EACH ROW EXECUTE FUNCTION relay_sample_source_changed();
CREATE TRIGGER relay_sample_note_changed AFTER UPDATE OF revision,forgotten OR DELETE ON relay_agent_knowledge
 FOR EACH ROW EXECUTE FUNCTION relay_sample_source_changed();

CREATE FUNCTION relay_sample_actor_changed() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF TG_OP='UPDATE' AND NEW.role IS NOT DISTINCT FROM OLD.role THEN RETURN NEW; END IF;
 UPDATE relay_agent_rehearsals SET state='invalidated',claim=NULL,lease_until=0,prepared=NULL,output=NULL,
  detail='Workspace membership changed. Start a new sample with current authority.',
  finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
 WHERE workspace=OLD.workspace AND actor=OLD.user_id AND state IN ('queued','running');
 RETURN CASE WHEN TG_OP='DELETE' THEN OLD ELSE NEW END;
END $$;
CREATE TRIGGER relay_sample_actor_changed AFTER DELETE OR UPDATE OF role ON relay_members
 FOR EACH ROW EXECUTE FUNCTION relay_sample_actor_changed();
