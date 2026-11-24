# Memory (schema 142)

Built 2026-11-24 from [the memory system proposal](../designs/memory-system/README.md).
Agents keep short notes of what they learn while working, and use them in later
requests. One engine holds four kinds of shelf; the rooms Cuckoo already has decide
which shelves a request may read and write.

| Shelf | Belongs to | Holds | Read in | Opened by |
| --- | --- | --- | --- | --- |
| **Workspace** | the workspace | facts about the business any agent needs | every shared agent's requests (not outside threads); assistants read it | admins, Settings › Memory |
| **Job** | one shared agent | what it learned about its own work | that agent's requests (not outside threads) | its owners and admins, its page › Knows › Memory |
| **Room** | one agent in one channel, group, work room or outside thread | what that room settled | that agent, in that room only | its owners and admins who can open the room, the same panel |
| **Person** | one agent with one person | how to work with that person | that agent, in that person's private conversation only | that person, the agent's page › Knows › About you |

A person's own assistant keeps its memory of its person as before
([assistants](assistants.md#memory-schema-138)); it reads the workspace's memory and
never writes to it. Agent notes and approved lessons stay rules a person wrote or
approved; a room's brief stays the short-term picture of a room.

## Which shelves a request uses

The controller decides from the running task (its conversation, agent, requester
and work class), never from tool arguments (`memory::access`):

| The request runs in | Reads, nearest first | Keeps "here" on |
| --- | --- | --- |
| A conversation one person reads (direct chat, a person's own group) | person, job, workspace | the person shelf |
| A channel or group two or more people read, or a work room | room, job, workspace | the room shelf |
| A thread with people outside (email, WhatsApp) | room only | the room shelf |
| A person's own assistant | workspace | nothing: it uses `remember_about_person` |

A note goes on the job or workspace shelf **at once** only when it was learned where
the whole workspace may read: a public channel, asked by a person, with no
restricted source. Otherwise it is saved **waiting** for a person, with a reason:
`private_source` (a private chat or a closed group), `outside`, `background` (a
schedule, trigger or work room) or `disagreement` (it corrects a note this request
may not change, or a pinned one). From a private place, a note that names a member
or carries an email address, a run of five or more digits or an amount is refused
outright, with the same check lessons use. A request built from restricted sources
(Library, connected events) keeps nothing. Passwords, keys, codes and card numbers
are refused everywhere.

## Agent tools

Served by `ayati_control` to agents on Cuckoo's executors; the arguments carry words
only (`crates/guest/src/memory_schema.rs` refuses any other field).

- `remember {text, kind, topic?, until?, for?, replaces?}`. `kind` is customer,
  system, people, decision, practice, preference, status or term. `for` is `here`
  (default), `my_job` or `workspace`. `until` is a date (YYYY-MM-DD, ahead, within
  two years). `replaces` names a note this one corrects: the correction is filed on
  that note's shelf and ends it, or waits as a disagreement. The same words on the
  same shelf are kept once; a second agent learning them adds 15 strength.
- `end_memory {memory, reason?}` ends a note on a shelf the request may change
  directly; otherwise it says to remember the correction instead.
- `search_memory {query}` searches every shelf the request may read, by words (any
  of them, each also as a start), ranked by match and strength: up to 15 notes and 3
  topic summaries, each labelled `workspace`, `my job` or `this conversation`. Found
  notes' fading clock restarts. Since schema 143 strength follows the answer's
  outcome instead ([evolution](evolution.md)): accepted +5, corrected −10, and the
  notes a correction shows wrong go on hold; with an embedding model and pgvector,
  notes close in meaning are found too. For an assistant it
  also returns its lines about its person.
- `search_conversation_history` (changed): see below.

## Given with every request

Without searching, a request is given the best notes of each shelf it reads, in a
fixed size however large the memory grows: the workspace 2,000 characters, the job
1,500, this room or person 1,500, and up to three topic summaries that share words
with the request, 3,000. Pinned notes come first, then essentials, then by strength
and use. When more exist the block says how many and points to `search_memory`. The
block is labelled information, never instructions; the agent's instructions add
where to look for what (notes for lasting facts, history for what was said, the
Library for documents, connected apps for live numbers) and to say so when nothing
is found. Both show in Agents › the agent › Cuckoo's rules as **Memory** and **What
it remembers**.

## Strength, fading and the nightly tidy-up

Every note has a strength from 0 to 100: 50 when an agent kept it, 90 when an admin
or owner wrote, confirmed or kept it; outcomes of the answers that used it change it
([evolution](evolution.md)). Unused, it halves once per half-life of its
kind, computed when read (`relay_memory_strength`): status 14 days, people 90 days,
customers, systems and preferences 180 days, decisions, practices and terms 365
days. Pinned notes never fade. Below 30 a note is listed as fading; below 10 it is
no longer given or found.

About once a day per workspace a controller task (`memory_tidy`) tidies every
shelf, never mixing shelves:

1. notes past their `until` date end;
2. the same note kept twice on a shelf becomes one (strongest + 15, uses added);
3. notes below 10 go to the archive;
4. archived and ended notes are deleted after a year;
5. with `AYATI_LEARNING_MODEL` set, up to five topics a day with 25 notes or notes
   older than 90 days get their summary written by that model, and their old
   unpinned notes fold into it (usage recorded as assignment `memory`).

Each change is recorded with the notes as they were (`relay_memory_changes`, kept 90
days) and listed under **Changes**, where an admin (workspace) or owner (job) can
undo it once.

## Screens

- **Settings › Memory** (admins; between Agent rules and Usage): counts, then
  **Needs your answer** (waiting notes with Keep / Discard, or for a correction
  Replace it / Keep the old note), then Topics (default), Notes, Essentials, Fading,
  Archive, Changes and **Looked for, not found** (searches with no result in the last
  30 days, from conversations the admin can open). Each note shows its kind, topic,
  who learned it, where (only when the viewer can open that conversation), when, how
  often it was used, and a five-step strength meter with a word. Actions: Pin,
  Confirm, Make essential, No longer true, Remove (asks first); archived notes:
  Bring back. Topics take a hand-written summary. Admins add notes.
- **Agents › the agent › Knows › Memory** (its owners and admins): the same browser
  for its job notes and its room notes in rooms the viewer can open. Never a
  person's notes.
- **Agents › the agent › Knows › About you** (anyone who talks to a shared agent):
  what it remembers about you, with Remove. Only you see it.
- **Activity › a request**: "Given from memory: 9 notes", "Searched memory: "Acme
  payment" · 3 found", "Searched this conversation: "2291" · 1 found", "Remembered
  for Meera Nair: …", "Suggested, waiting for a person, for the workspace: …",
  "Ended a note: …". A removed note shows as "(since removed)".

## History search (changed)

`search_conversation_history {query, before?, from?, since?, until?}` now searches
the whole eligible conversation with the message index of schema 92: messages with
all the words (each also as a start) newest first, 20 a page with `next_before`;
when none has every word, the 20 best with any word (`matched: "any word"`). `from`
is an author ID, `since` and `until` dates. A query with no words (only symbols) is
matched literally. Scope, exclusions, since-joining and the reply-sequence bound are
unchanged ([conversation context](conversation-context.md#older-history-tools)).
Each search with words is recorded for Activity.

## API

Workspace (admins) and agent (its owners and admins) pages share one shape:

- `GET …/workspaces/{w}/memory?view=topics|notes|essentials|fading|archive|waiting|changes|missed&q=&topic=`
  and `GET …/agent-profiles/{a}/memory?…` return `{available, view, items, counts, used_month, summaries}`.
- `POST …/memory` (or `…/agent-profiles/{a}/memory`) with `{id, kind, topic, text, until}`
  adds a note or rewords one on the page (the same id saves once).
- `POST …/memory/notes/{id}/{pin|unpin|confirm|essential|unessential|end|restore|remove|keep|discard}`.
- `POST …/memory/topics` with `{topic, summary}` (empty removes it).
- `POST …/memory/changes/{id}/undo`.
- `GET …/agent-profiles/{a}/memory/about-me` and `POST …/about-me/{id}/remove`: the
  viewer's own notes only.

Tables: `relay_memories` (with the generated `shelf` key and foreign keys that
delete a person's, agent's or conversation's notes with them),
`relay_memory_topics`, `relay_memory_uses`, `relay_memory_changes`,
`relay_memory_tidy`; functions `relay_memory_half_life_ms` and
`relay_memory_strength`. Apply `m20261124_000142_memory` with the administrator
migration procedure before starting this version; it adds tables only.

## Checks

- PostgreSQL `memory::tests`: Meera's note never reaches Ravi's chat or the
  channel; a workspace note from her private chat waits, one naming a person is
  refused, a password is refused, a forged `chat` argument fails; the channel's
  notes reach the workspace and its room at once and stay out of private chats; what
  each request is given and searched is recorded for Activity.
- `memory::manage_tests`: members can't open the workspace's or an agent's memory;
  the admin's waiting list hides where a private note came from; keeping it makes it
  strong; "looked for, not found" shows only searches from conversations the admin
  can open; the agent page shows job and room notes, never a person's; About you is
  the person's alone; an agent can't end a workspace note from a private chat and its
  correction waits; keeping it ends the old note; pin, end, restore, remove and
  dates.
- `memory::tidy_tests`: expiry, merging, archiving, a pinned note never fading,
  deletion after a year, once a day; undo of a merge, a deletion and a summary;
  folding old notes into a summary; a hand-written summary; a stranger refused.
- `conversation_history_tests` (updated): words found across the whole conversation
  in one search, word starts, the any-word fallback, a literal query, author and date
  filters, the Activity line.
- Guest: the memory tools carry no person, room, agent or shelf; history accepts
  only its new fields. Unit: dates and secrets.
- Web `tests/memory.test.mjs`: one answer for a double click, the waiting list and
  topics refreshing, opening a topic, removing asks first, a failed change keeps the
  note and says why, a member refused, a retried note keeps its id, About you. The
  Settings and agent page tests list the new page and chips. The screens were looked
  at in a browser with sample answers, light and dark, desktop and phone.

**Not run with a real model**: whether agents keep the right notes, choose the right
shelf and use what they are given is unproven, as is the quality of model-written
topic summaries.

## Limits

- Since schema 143 ([evolution](evolution.md)): meaning search with an embedding
  model and pgvector; the classifier checks each new note for duplicates and
  contradictions among nearby notes; corrections hold the notes they show wrong; and
  strength follows outcomes.
- Notes are never marked safe for people outside the workspace, so outside threads
  get only their own notes.
- Owners and admins don't get a page of other people's person shelves; they see
  those notes only as lines on requests they may already open in Activity.
- Removing a note does not remove what was already said in messages, replies or
  backups.
