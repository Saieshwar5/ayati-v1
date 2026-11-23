-- Schema 141: one position per chat for a member's assistant.
--
-- In a direct chat a member chooses what their assistant does there: nothing
-- (no running thread), write each reply for them to send (`ask`), or send
-- replies as them (`auto`, what a thread did before). `draft` is the reply the
-- assistant wrote in its own room that waits for the member, in `ask` mode.
ALTER TABLE relay_work_threads
 ADD COLUMN mode TEXT NOT NULL DEFAULT 'auto' CHECK(mode IN ('ask','auto')),
 ADD COLUMN draft TEXT;
