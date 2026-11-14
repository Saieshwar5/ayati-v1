-- WhatsApp removed (schema 113). Agents keep Cuckoo Chat and email as their
-- native channels; people reach agents on phones through Cuckoo's own app.
-- Schema 111's tables go. Any live WhatsApp endpoint is switched off as a new
-- generation, so its conversations close and replies not yet sent never go.
UPDATE relay_communication_endpoints SET enabled=FALSE,revision=revision+1
 WHERE transport='whatsapp' AND NOT simulated AND enabled;
UPDATE relay_communication_outbox o SET state='cancelled'
 FROM relay_communication_endpoints e
 WHERE e.workspace=o.workspace AND e.id=o.endpoint AND e.transport='whatsapp' AND NOT e.simulated
   AND o.state IN ('queued','failed','uncertain');
DROP TABLE relay_mobile_link_codes;
DROP TABLE relay_user_mobiles;
DROP TABLE relay_whatsapp_numbers;
