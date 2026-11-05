# Library and agents

How agents use the [Library](library.md): folders and their sharing, agent access and
"use for" audiences, the agent tools, passages and word, meaning and image search,
always-read documents, agents suggesting, editing and saving, and how Library changes
fence agent work. Built 2026-11-05 from the Library plan (slices LB1–LB3, draft decision
D242), schemas 69–74.

## Folders and agents answering from them (schema 69, LB1)

Built 2026-11-05 from the Library plan (slice LB1, draft decision D242).

**Folders.** One level: a folder holds pages and files, not other folders. Anyone can
create one (`POST /api/identity/workspaces/{w}/library-folders` with `{id, request,
title}`); it is private to its creator until shared. Everything in a folder follows the
folder's sharing: people (Only me, Selected people, Workspace; Can view or Can edit) and
agents. An item's own steward keeps access to it. Library → All shows one level, folders
first with their item count; `?folder=` lists one folder's items; other views and
searches show items wherever they are filed, with the folder's sharing and name.

- People who can edit a folder can create pages in it (`folder` on the first save) and
  file their own items into it. **Move** (`POST …/library/{id}/move` with `{request,
  revision: access_revision, folder | null}`) is for the item's creator. Moving in clears
  the item's own sharing; moving out makes it private again. Both are audited in plain
  words and fence work that used the item.
- An item in a folder can't be shared on its own ("share the folder or move the item
  out"). Sharing a folder raises the access revision of every item in it, so source
  receipts see the change.
- A folder goes to Trash only when empty. An item restored from a folder that is in
  Trash comes back unfiled and private.

**Agent access per folder or item**, set by whoever manages its sharing, separately for
each agent (`relay_library_agents.level`, `use_for`):

| Use for | The agent may use it when… |
| --- | --- |
| **People who can see it** (default) | everyone who will read the answer can open it: the asker, everyone in the conversation (every member for a public channel) and a schedule's posting conversation |
| **All staff** | any colleague asks, without them opening the document |

Customer answers ("Customers") wait for customer mode; replies that go outside the
workspace through the communication gateway can use neither. The only level offered is
**Read**: Suggest, Edit with approval, Edit and Save here are reserved for agent editing
(LB3) and refused until then. The server applies all of this (`relay_library_agent_use`),
never the agent. Sharing changes are audited with each agent's audience ("Changed sharing
of "HR": Nova reads it for people who can see it").

**Agent tools** (ayati_control, pooled executors):

- `list_folder({folder?})`: the folders and unfiled documents this agent may use here, or
  one folder's documents.
- `search_library({query, folder?})`: passages, not whole documents (LB2a, below): up to
  10, at most 3 from one document, each with its document, folder, section, passage
  number and a short preview with the matching words in bold.
- `read_document({id, passage?})`: whole passages from that one on, about 6,000
  characters at a time (`next_passage` continues); the first read lists the document's
  sections. Anything the agent can't use for these people is "not found", without saying
  whether it exists.

Each document shown leaves a receipt (`relay_library_reads`: reply, item, revision,
access revision, whether it was read and the section read). Search results count too: a
preview is content. **Before the reply is published**, every document shown must still be usable
for that audience with unchanged sharing, or the reply is withheld ("Request cancelled
because access changed"), like Ask. The finished reply ends with **Library documents read
for this answer**, links Cuckoo makes from its receipts, not the model, naming the section
read ("Support handbook › Refunds · revision 3"); documents only previewed by search
aren't cited. Chat opens them in the same tab at the cited revision. Cited revisions are kept with page history.
Agents given anything in the Library get one instruction line about these tools.

### Passages and full-text search (schema 70, LB2a)

Every page and file is kept cut into **passages** (`relay_library_passages`) of about a
paragraph or a short section (closing at a line break after 1,200 characters, never over
2,000; up to 300 per item), each with the heading path it sits under ("Returns ›
Exceptions"). Pages are read from their headings; file text uses Markdown heading lines;
PDF page breaks start "Page N" sections.

- Saving a page, restoring a version or an item, and finishing an upload index the item
  in the same transaction. Items indexed before this, or missed, are caught up 100 at a
  time (newest first) before an agent searches, and a stale item is indexed when read.
  Index state lives in `relay_library_indexed`, apart from the items, so indexing is not
  an access change and doesn't fence earlier answers.
- Search is PostgreSQL full-text search with English word stems ("refunded" finds
  "refund"), matching any of the question's words and ranking title above section above
  text. Only passages of an item's current revision are searched. Meaning-based search
  (embeddings) and per-document summaries are LB2b.
- **Word (.docx)** and **Excel (.xlsx)** uploads are recognised by their contents and read
  in process with size limits (at most 4,000 parts and 96 MiB unpacked, 7 seconds, 64,000
  characters kept): Word headings (Title, Heading 1–3) become sections, paragraphs and
  table rows text; each Excel sheet is a section with one line per row labelled by the
  first row ("Product: Desk · Price: 1250 · Updated: 2025-11-05"), dates as dates, up to
  2,000 rows per sheet. They download as attachments, never inline. Files uploaded before
  this keep their earlier readiness; upload them again to make them readable.

### Meaning search and summaries (schema 72, LB2b)

Optional, configured on the controller with the model gateway's providers file:

```sh
AYATI_MODEL_PROVIDERS_FILE=/private/model-providers.json
AYATI_LIBRARY_EMBEDDING_MODEL=openai:text-embedding-3-small   # provider:model
AYATI_LIBRARY_SUMMARY_MODEL=openai:gpt-5-mini                  # provider:model
```

- **Embeddings** need the pgvector extension (tested with 0.6.0 on PostgreSQL 16). The
  migration installs it into `public` when the server offers it and creates
  `relay_library_embeddings`; Cuckoo names its type and operator in full, so any schema
  can use it. Without pgvector, or without an embedding model, word search works alone.
  The model must return 1,536 numbers per text.
- The **indexer** is a controller task: every few seconds it embeds up to 64 waiting
  passages of one workspace (title and section included) and writes up to 4 one-line
  summaries (`relay_library_summaries`, under 300 characters, made from the first 6,000
  characters). Only current revisions are embedded or summarised; re-cutting a document's
  passages drops their embeddings and summaries follow the new revision. Saving never
  waits. Tokens are recorded in model usage (`assignment` "library", per workspace).
  Executors are never involved and never see keys.
- **Search** asks the embedding model for the question's meaning before taking the
  workspace lock, then merges word and meaning matches by rank (reciprocal rank fusion);
  meaning matches farther than cosine distance 0.75 are dropped. Matching is an exact
  scan of the agent's usable documents in that workspace, bounded by the 1,000-item
  limit; no approximate index. If the model is down, word search answers alone and the
  result says `by_meaning: false`. Every meaning match passes the same audience rules.
- Search results and `list_folder` include each document's summary when one exists for
  its current revision. The text of every document is sent to the configured provider
  for embedding and summarising, like any model request.

### Text in images and "always read" documents (schema 73, LB2c)

- **Images** (PNG, JPEG, up to 40 megapixels) are read by text recognition: Tesseract
  (English) in the same sandbox as PDF text, with 1 GiB of memory, 20 CPU seconds and
  25 seconds at most. The text becomes one "Text in the image" section, marked
  "partial" (recognition can be wrong); images without readable text stay downloadable.
  Needs `tesseract-ocr` and `tesseract-ocr-eng` beside `bwrap`, `prlimit` and `pdftotext`.
- The sandbox's `/lib64` now follows the host (`usr/lib64` on Debian and Ubuntu,
  `usr/lib` elsewhere). Before, every sandboxed reader, PDF text included, failed on
  Ubuntu x86-64 and files stayed "unsupported".
- **Always read**: on an agent's Instructions tab, its owner or an admin chooses up to 5
  documents it was given, in order (`GET/POST /api/identity/workspaces/{w}/library-always/{agent}`
  with `{items, expected}`; a save from an older list is refused). People who can talk
  to the agent see the list; documents they can't open are counted, not named. Changes
  are in the Audit log ("Nova always reads: Returns policy, Tone guide").
- Each request includes the chosen documents that are usable for its audience, by
  section, within 8,000 characters together, in the agent's instructions as evidence.
  They leave the same receipts as search previews, so a sharing change stops or hides
  the work, and they are not cited unless the agent also reads them.

### Agents suggesting, editing and saving (schema 74, LB3)

An agent's level on a folder or item, set in Share, each including the ones before:

| Level | The agent can |
| --- | --- |
| **Read** | search and read (files offer only this) |
| **Suggest edits** (default for new grants) | `suggest_edit`: a page's new text and a note, waiting for a person |
| **Edit** | `edit_document`: a new version, given the revision it read (a newer page is not overwritten) |
| **Edit and save here** (folders) | `save_document`: a new page in the folder, with its link |

"Edit with approval" stays reserved. Every change also needs the person the agent works
for to be allowed it (open the page to suggest, edit it to edit, edit the folder to save)
and "use for" to fit the conversation; otherwise the tool says not found or not allowed.
Agents write plain text with Markdown headings and lists, turned into the page format and
checked like any page (60,000 characters at most). Agents never change who has access.

- Edits and saves are versions authored by the person and marked with the agent and task
  (`relay_library_versions.agent`, `job`); History shows "by Nova", and restoring an
  earlier version is the undo. They are indexed at once and audited ("Nova edited
  "Returns policy" (revision 3)", "Nova saved "Quote for Acme" in "Sales"").
- **Suggestions** (`relay_library_suggestions`, up to 20 waiting per page) show on the
  page as "Nova suggested a change · Review": the note, who asked, and the current and
  suggested text side by side. Someone who can edit accepts (a new version marked with
  the agent) or rejects; a suggestion made on an older revision can only be rejected.
  `GET …/library/{id}/suggestions`, `POST …/library/{id}/suggestions/{s}` with
  `{request, action}`; decisions are audited and replay-safe.

**Fencing (schema 71).** Library edits and sharing changes no longer raise the
workspace-wide context epoch, which used to hide every earlier agent answer in the
workspace and stop running work whenever anyone saved a page. Instead an answer that
read or was shown a document is hidden in Chat ("Response unavailable because source
access changed"), left out of agents' conversation history and stopped while running
only when that document's sharing changes or it goes to Trash. Content edits never fence;
citations keep naming the revision read.

Limits: an answer delivered to a conversation stays there if someone joins later; the
first release lists up to 100 documents per folder and offers up to 50 folders when
moving; uploads land unfiled and are moved afterwards.

## Recorded verification — 2026-11-05 (LB1)

- PostgreSQL (`postgres/library_folder_tests.rs`): items follow their folder's sharing and
  its access revision; items in folders can't be shared apart; only folder editors add
  to a folder; move in and out clears sharing; a non-empty folder can't go to Trash and a
  restored item comes back unfiled and private; moves are audited. A colleague without
  access gets the all-staff policy from `search_library`/`read_document` and a cited
  answer, while the HR folder is invisible to them; the owner may use HR privately but
  not in a conversation with the colleague; changing HR's sharing after a read withholds
  the reply. Editing levels, customer and unknown audiences are refused; Ask follows the
  folder's agents. The existing Library, source and upgrade tests pass.
- Real Goose (`tests/library_folders.mjs`): an admin creates a folder in the browser,
  shares it with an agent for all staff and sees it in the folder header; a colleague's
  question is answered with a "Library documents read" citation that opens the page;
  an HR document is never read for them or where they will read the answer. Screenshots
  at desktop, 390 px (no horizontal overflow) and dark.
- Web (`tests/library-folders.test.mjs`, `library.test.mjs`, `markdown.test.mjs`): folder
  sharing with Can edit and the safest audience by default; one folder per click burst
  with the same identity on retry and the name kept after a failure; Move offers only
  editable folders and sends the sharing revision; an open folder lists its own items;
  Library citation links open in the same tab and other relative links stay text.


## Recorded verification — 2026-11-05 (LB2a)

- Unit: passages follow headings and page breaks, cut near 1,200 characters and never
  past 2,000 without losing text; pages are read from their structure; Word headings,
  paragraphs, tables and entities and an Excel sheet with numbers and a date become text;
  other zips and bad bytes are refused; search keeps three passages per document and
  reads whole passages with an outline.
- PostgreSQL (`postgres/library_passage_tests.rs`): a colleague's stemmed question finds
  the handbook's Refunds section (not the HR folder), within a folder too; reading lists
  sections and returns the section; the answer cites "Support handbook › Refunds" and not
  a document only previewed; an edit re-indexes and the next read has the new text and
  revision. Items from before indexing, including an Excel file, are caught up on search
  without changing the workspace's context fence. A search preview alone fences the reply
  when sharing changes; a new page is searchable as soon as it is saved.
- Real Goose (`tests/library_folders.mjs`, last part): a Word file uploaded and filed in
  the shared folder is found by section and the answer cites "Warranty.docx › Desks".


## Recorded verification — 2026-11-05 (schema 71 fence)

- PostgreSQL (`postgres/library_fence_tests.rs`): editing a policy and re-sharing an
  unrelated folder change no context epoch and keep both a Library-based and a plain
  answer in Chat and in agents' history; re-sharing the policy's folder hides only the
  answer that read it, in Chat and in history. A running agent keeps working through an
  edit and is stopped when the document it read goes to Trash. The full PostgreSQL suite
  (368) passes, including every earlier revocation test.
- Real Goose (`tests/library_folders.mjs`): the first answer is still readable after a
  Word file is uploaded and moved into the folder.


## Recorded verification — 2026-11-05 (LB2b)

- PostgreSQL (`postgres/library_meaning_tests.rs`, pgvector 0.6.0) with a loopback
  provider: the indexer embeds every passage and summarises both documents; "can I send
  something back?" finds the Returns policy by meaning although it shares no word with
  it, with its summary, and never the HR folder; without the model the same question
  finds nothing; usage is recorded; an edit drops old embeddings and the summary
  follows the new revision. Config: provider:model parsing and the providers-file rule.
- Real Goose (`tests/library_folders.mjs`): the controller indexes through the providers
  file; the agent's search for "Can I send something back?" returns the policy by
  meaning with its summary, and indexing usage is recorded. The fixture's embeddings
  are deterministic word groups, not a real model: this proves the plumbing, not
  retrieval quality.


## Recorded verification — 2026-11-05 (LB2c)

- Sandbox (`library/extract.rs`, needs bwrap and tesseract): a receipt photo's text and a
  two-page PDF's text are read; broken image bytes are unsupported.
- PostgreSQL (`postgres/library_always_tests.rs`): only managers choose, only documents
  given to the agent, at most five, never from an older list; the Audit log names them;
  a colleague's request gets the staff policy and not HR; answers don't cite them; the
  owner's private request gets both and stops when HR is shared differently; a colleague
  who can open neither sees only the count.
- Web (`tests/library-always.test.mjs`): add, order, remove, the character budget, one
  save per click burst with the expected list, the draft kept after a failed save, and
  a read-only list naming only what the viewer can open.
- Real Goose (`tests/library_folders.mjs`): a receipt photo uploaded and filed is found
  by its recognised text; the owner chooses an always-read policy on Iris's Instructions
  tab in the browser, and the next request carries it uncited and never HR.


## Recorded verification — 2026-11-05 (LB3)

- Unit: agent text with headings, bullet and numbered lists and paragraphs becomes a
  page that passes page validation.
- PostgreSQL (`postgres/library_edit_tests.rs`): a colleague who can't open the policy
  can't have it changed; suggest level can't edit; suggestions wait (nothing changes),
  are listed with agent and note only for people who can open the page, are rejected or
  accepted once into a version marked with the agent, and a stale one can only be
  rejected; edit level refuses an outdated revision, writes a searchable version, can't
  save; restoring is the undo; save level files a page in the folder as the person,
  marked with the agent; HR can't be edited where a colleague without access reads;
  "save here" on an item and the reserved level are refused.
- Web (`tests/library-edits.test.mjs`): the suggestions bar and review, one decision per
  click burst with the same request on retry, nothing changed on failure, stale ones
  only rejectable, read-only review, no bar without suggestions; levels offered for
  folders, pages and files.
- Real Goose (`tests/library_folders.mjs`): Iris suggests an edit, the owner reviews and
  accepts it on the page and History shows Iris; Iris edits the policy and saves a quote
  in Policies, both audited.


## Dependency decisions

- calamine 0.36.1 (MIT), with zip 8.6.0 (MIT, deflate only) and quick-xml 0.41.0 (MIT;
  already in the tree via object_store): Excel cells with their types for LB2a, and Word
  text from the .docx zip with the same versions. Pure Rust, in process, nothing
  executed. Chosen over newer releases that were days old; calamine's chrono feature is
  off and Excel date serials are converted by Cuckoo.
- pgvector 0.6.0 (PostgreSQL License), the server extension; no Rust crate is added
  (vectors travel as text and are cast in SQL).
- Tesseract 5.3.4 (Apache-2.0) with English data, run as a local program in the existing
  sandbox; no Rust crate is added.
