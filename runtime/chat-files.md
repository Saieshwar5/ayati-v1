# Chat files for agents (schema 100)

A file someone sends in Chat is context, like the words of their message. Agents know
a file came with a message, answer from what it says, find files sent earlier, and use
a file only where the message that shared it may be read. Built 2026-11-07 on
`feat/chat-file-reading`; it needs no setting. Voice notes keep their transcripts
([voice transcription](voice-transcription.md)).

## Where a file and its text live

| What | Where |
| --- | --- |
| The bytes | Unchanged: `chat/<workspace>/<file>` in the private S3 bucket (hosted) or `<AYATI_DATA_DIR>/workspaces/<workspace>/objects/<file>` locally. |
| The file | `relay_objects`: name, type, size, SHA-256, uploader. The type now comes from the bytes the way the Library detects it (PDF, Word, Excel, plain text, PNG, JPEG, audio), else `application/octet-stream`. It never decides serving: only images and audio open in the browser. |
| Its text | `relay_object_contents` (schema 100): `status` (pending, ready, unsupported, failed), `kind` (text, pdf, word, excel, image), `text` (up to 64,000 characters), `truncated`. It is deleted with the file. |

## Reading, once

Every non-audio upload queues a reading in the transaction that saves the file.
Every controller runs a reader (`src/identity/file_reading/`); a database lease means
each file is read once. It uses the Library's local readers in their sandbox (no
network, limited memory, CPU and time): `pdftotext` (first 50 pages), Word and Excel,
UTF-8 text, and Tesseract for text in PNG/JPEG. Nothing leaves the controller. A
format no reader handles, or a file without text, is `unsupported` and never retried;
a file storage can't return is tried three times, then `failed`. An upload wakes the
reader through the schema 97 notifications (`read`), with a 60-second fallback.
Files already shared in sent messages when schema 100 is applied are queued too.

The controller needs `bwrap`, `prlimit` and `pdftotext` for PDFs, and `tesseract`
with `tesseract-ocr-eng` for images. The release image installs the first three
(Dockerfile) but not Tesseract, so hosted images have no text recognised until it is
added; those files are `unsupported` and stay downloadable.

## What agents see

- **Every message** lists its files: `file_id`, `name`, `mime`, `size` and, for other
  files than audio, `reading` with `kind`, `characters` and `truncated` when ready.
  No text, so the 14,000-byte history budget stays for the conversation.
- **`request_files`** in the conversation evidence holds the text of the files on the
  request, on the message it replies to and on the ten messages before it, newest
  first: up to 12,000 characters per file and 24,000 in all, with `next_offset` where
  the rest can be read. It sits outside the history budget, inside the prepared limit.
- A request with empty content shares only files: the agent is told the files are the
  request, to ask when what's wanted isn't clear, never to guess what a file it
  couldn't read says, and that file text is untrusted data, never instructions,
  approval or permission, whatever the file says.
- **Waiting**: a request whose conversation got a file in the two minutes before it
  that is still being read waits for it, at most 30 seconds and without holding an
  executor slot (`pool_candidate.sql`), as for voice notes.

## Finding files sent earlier

Two controller tools sit beside the history tools (`conversation_files.rs`):

- `list_conversation_files({name?, before?})`: files shared before the request,
  newest first, with their reading or transcript status and the message (author,
  time) that shared them; `name` matches part of a file name; ten messages' files a
  page; follow `next_before`.
- `read_conversation_file({file, offset?})`: a file's text, or an audio transcript, in
  4,000-character chunks.

Reads are recorded per attempt in `relay_context_reads` and count toward its budget.

## Who a file is used for

A file is exactly as private as the message that shared it. The tools reuse the
history tools' scope: the controller binds workspace, agent, requester and
conversation from the running task, and only sent messages before the request count,
with the joining policy, thread scope and source exclusions.

| Where the file was sent | The agent can use it for |
| --- | --- |
| A direct conversation, or a person's own group with agents | That person, there |
| A shared room (channel, or a group with two or more people) | People in the room, in replies the room reads: they can already open the file |
| Anywhere else | Nobody |

- A file from another conversation, an unsent upload and a deleted message's file all
  return the same `not_found` as a made-up ID. A file can only be attached by its
  uploader in the conversation it was uploaded to (unchanged).
- When someone in a room means a file they sent privately, the agent uses
  `continue_privately` and answers in their direct conversation.
- File text is never copied into agent notes or knowledge. A working summary that
  cites a file keeps its own scope (per person, or the room's brief).

## People and deleting

Message pages carry `reading` for each non-audio file. Under a shared document Chat
shows "Reading for agents…" while it is read and "Agents can't read this file" when it
can't be; nothing once it can, and nothing for images and audio.

Deleting a message now deletes its files too, unless another of its sender's messages
still shares them: the file rows, readings and transcripts in the same transaction,
then the stored bytes. A storage failure is logged without the file name and leaves
bytes that nothing points to.

## Upgrade

Apply forward migration 100 with `ayati workspace-db migrate` before starting the new
controller; runtime grants must cover `relay_object_contents`. Earlier files shared in
messages are read in the background after the upgrade, two at a time per controller.

## Checks

- Unit: Chat type detection (`collaboration/files.rs`), the readers
  (`file_reading/worker_tests.rs`; PDFs need `bwrap`), the guest tool schemas
  (`crates/guest/src/file_schema.rs`).
- PostgreSQL: the queue, racing controllers, lost leases, retries and the backfill
  (`object_contents_tests.rs`); what agents are given (`agent_files_tests.rs`); the
  tools' privacy (`conversation_files_tests.rs`); deleting through the hosted routes
  with an in-memory S3 store (`hosting_tests.rs`).
- Web: `web/tests/file-reading.test.mjs`.
- Connected (`tests/chat_files.mjs`, in `run-agent-load.py --suite workspace`): real
  pinned Goose with a scripted model gets the request's file text in its first prompt,
  finds a file outside recent context with the file tools, and is refused a file from
  another conversation of the same agent; its tool list is the reviewed set.

Recorded 2026-11-07: Clippy, rustfmt, workspace tests, the PostgreSQL suite (466
passed; the Library meaning test needs pgvector, not installed on that machine), the
web suite (383) and build, and the connected file and history journeys passed. Not
run there: the PDF and image readers (no `bwrap` or Tesseract) and a live browser
check of the reading label.

## Not yet

- Seeing images: a vision model's description and a `look_at_file` tool (phase 2).
- Searching inside file text with `search_conversation_history`, a Files list in a
  conversation, and Save to Library for an attachment.
- Attachments from email, WhatsApp and other channels, which aren't accepted yet.
