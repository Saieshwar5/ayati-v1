-- Schema 102: web search for agents. One row per search or page read, for the reply
-- that asked. Cuckoo cites the pages read under the answer, counts each request's
-- searches and reads against its limits, and lets an agent open only addresses it
-- found or a person gave. `provider` and `cost_usd` are for operators and usage;
-- workspaces never see which service answered.
CREATE TABLE relay_web_reads (
 workspace TEXT NOT NULL,
 reply TEXT NOT NULL,
 position INTEGER NOT NULL CHECK(position >= 0),
 agent TEXT NOT NULL,
 kind TEXT NOT NULL CHECK(kind IN ('search','read')),
 -- The words searched for, or the page's address.
 query TEXT NOT NULL CHECK(char_length(query) BETWEEN 1 AND 2048),
 title TEXT NOT NULL DEFAULT '' CHECK(char_length(title) <= 300),
 -- A search's result addresses and titles: [{"url":…,"title":…}].
 results JSONB NOT NULL DEFAULT '[]'::jsonb CHECK(jsonb_typeof(results)='array'),
 -- A page's text, kept for this reply so reading on costs nothing more.
 text TEXT NOT NULL DEFAULT '' CHECK(char_length(text) <= 60000),
 provider TEXT NOT NULL,
 cost_usd NUMERIC(12,6) NOT NULL DEFAULT 0 CHECK(cost_usd >= 0),
 at BIGINT NOT NULL,
 PRIMARY KEY(workspace,reply,position),
 FOREIGN KEY(workspace,reply) REFERENCES relay_chat_messages(workspace,id) ON DELETE CASCADE
);
CREATE INDEX relay_web_reads_agent ON relay_web_reads(workspace,agent,at);
