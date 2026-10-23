ALTER TABLE relay_executor_slots ADD COLUMN preparation_version INTEGER NOT NULL DEFAULT 0
 CHECK(preparation_version IN (0,1));
-- Old ready sessions have not acknowledged generic initialization. Existing
-- offered/running assignments and their capabilities remain valid.
UPDATE relay_executor_slots SET state='preparing',ready_ms=NULL WHERE state='ready';
-- An older controller may still report during a rolling upgrade. Normalize its
-- ready reports too, without rejecting reports for already owned assignments.
CREATE FUNCTION relay_require_harness_preparation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 IF NEW.state='ready' AND NEW.preparation_version=0 THEN
  NEW.state='preparing';
  NEW.ready_ms=NULL;
 END IF;
 RETURN NEW;
END;
$$;
CREATE TRIGGER relay_harness_preparation BEFORE INSERT OR UPDATE ON relay_executor_slots
 FOR EACH ROW EXECUTE FUNCTION relay_require_harness_preparation();
DROP INDEX relay_slots_ready;
CREATE INDEX relay_slots_ready ON relay_executor_slots(state,provider,model)
 WHERE preparation_version=1;
