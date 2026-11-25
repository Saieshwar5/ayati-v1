-- Schema 157: an unanswered question is counted once per request, however many wordings
-- the agent tries, and not at all when a later wording in the same request finds something.
ALTER TABLE relay_library_gaps ADD COLUMN reply TEXT;
