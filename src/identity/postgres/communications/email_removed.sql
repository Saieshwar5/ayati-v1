-- Agent email removed (schema 131). Agents work in Cuckoo Chat; the version
-- with email is kept at the Git tag `agent-email-last`. Schema 113's tables
-- go. Any live email endpoint is switched off as a new generation, so work
-- that came by mail loses that door and replies not yet sent never go. What
-- people and agents wrote stays in their direct conversations.
UPDATE relay_communication_endpoints SET enabled=FALSE,revision=revision+1
 WHERE transport='email' AND NOT simulated AND enabled;
UPDATE relay_communication_outbox o SET state='cancelled'
 FROM relay_communication_endpoints e
 WHERE e.workspace=o.workspace AND e.id=o.endpoint AND e.transport='email' AND NOT e.simulated
   AND o.state IN ('queued','failed','uncertain');
DROP TABLE relay_email_messages;
DROP TABLE relay_email_addresses;
