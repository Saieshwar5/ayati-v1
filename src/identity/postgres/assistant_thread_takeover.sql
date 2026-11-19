-- Schema 136: handing a chat over from a message.
--
-- A member points at a message the other side sent in a direct chat and says
-- what their assistant should do. The thread starts running at once: nothing
-- is sent for the member first, and the assistant's first reply answers from
-- that message (`anchor`) on, following `instruction`.
ALTER TABLE relay_work_threads
 ADD COLUMN instruction TEXT NOT NULL DEFAULT '',
 ADD COLUMN anchor TEXT;
