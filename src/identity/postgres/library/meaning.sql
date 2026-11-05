-- Schema 72 (Library LB2b): one-line summaries per document and, where pgvector is
-- installed, passage embeddings for meaning search. pgvector lives in "public" and is
-- named in full, so every workspace schema can use it; without it word search remains.
CREATE TABLE relay_library_summaries (
 workspace TEXT NOT NULL, item TEXT NOT NULL, revision BIGINT NOT NULL,
 model TEXT NOT NULL, summary TEXT NOT NULL CHECK(char_length(summary) BETWEEN 1 AND 300),
 PRIMARY KEY(workspace,item),
 FOREIGN KEY(workspace,item) REFERENCES relay_library_items(workspace,id) ON DELETE CASCADE
);
DO $$
BEGIN
 IF EXISTS(SELECT 1 FROM pg_available_extensions WHERE name='vector') THEN
  CREATE EXTENSION IF NOT EXISTS vector SCHEMA public;
  EXECUTE format('CREATE TABLE relay_library_embeddings (
   workspace TEXT NOT NULL, item TEXT NOT NULL, position INTEGER NOT NULL,
   revision BIGINT NOT NULL, model TEXT NOT NULL, embedding %I.vector(1536) NOT NULL,
   PRIMARY KEY(workspace,item,position),
   FOREIGN KEY(workspace,item,position) REFERENCES relay_library_passages(workspace,item,position) ON DELETE CASCADE)',
   (SELECT n.nspname FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='vector'));
 END IF;
END $$;
