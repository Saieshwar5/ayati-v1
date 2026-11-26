# Library: one place for files (plan and status)

The direction agreed on 2026-11-25. The Library keeps, finds and controls access to every
file and knowledge asset of an organization; agents do the work on their computers. This
page lists what is built and what is next. [Library](library.md) and
[Library and agents](library-agents.md) describe current behavior.

## The rules

1. **Everything is a Library item**: uploads, chat attachments and what agents make.
2. **Access comes from the folder** an item is in.
3. **Agents are shared with like people**: Can view or Can edit, with "ask me first".
4. **Library keeps, computer works**: an agent opens an item on its computer and saves it
   back as a new version.
5. **Admins can open everything**; opening what was not shared with them is audited.

Decisions: admins' access includes "Only me" items. Every agent has a computer. Each
agent has its own folder with one subfolder per conversation, shared with that
conversation's people. Attachments in conversations without an agent stay out of the
Library. Files keep their latest 20 versions. Meaning search (vectors) is off by default
and turned on per folder by its manager; an agent may propose it. The server keeps
checking who will read an answer before an agent may use a document for it.

## Status

| Part | What | Status |
| --- | --- | --- |
| A1 | File versions and one history (schema 152) | Built and tested |
| A2 | Folders inside folders (schema 153) | Built and tested |
| A3 | Admins can open everything, audited (schema 153) | Built and tested |
| B4 | Open on the computer and Save back (schema 154) | Built and tested with the stand-in computer |
| B5 | Five agent tools instead of ten | Next: `open_document` and `save_file` exist beside the old tools |
| B6 | A computer for every agent | Built: on by default, released after use unless `keep_between_requests` |
| C7 | Each agent's own folder | Built, then removed (schema 159): files belong to the conversation |
| C8 | Conversation files | Built and tested: agents list, open and attach; people see Files in a conversation and on the agent's page, and Save to Library |
| D9 | Exact search mode | Built and tested (`exact: true`); folder, kind and date filters not yet |
| D10 | Meaning search per folder, off by default (schema 156) | Built and tested; agents propose, the manager decides |
| D11 | Descriptions instead of automatic summaries (schema 158) | Built and tested: a description line by editors and by agents in save_file, searched with the text |
| D12 | Unanswered questions counted per question (schema 157) | Built and tested |
| E13 | One share dialog for people and agents | Partly: agents are given access in people's words; people and agents are still two lists |
| E14 | Folder tree, filters, one details panel | Built: three places, a folder tree, filters, a details panel with About, Sharing, History and Activity; Move offers any folder |
| F15 | Keeping rule on agents' folders | Removed with the agents' folders |
| F16 | Files above 25 MiB | Not started (the item limit is now 10,000) |
| F17 | Remove the old tools and paths | Not started |

## Also built

- **Each item keeps its sharing source** (schema 160), kept by a trigger. With 10,000
  items four folders deep, as a member: home 50 ms, a folder 165 ms, search 317 ms,
  Shared 573 ms.
- **Admin audit**: looking inside a private folder and reading an item's history are
  audited, as opening and downloading were.
- **The instructions preview** includes Pages, as a direct request does.

## Left to do

1. **Run it with a real model and a real provider computer.** Nothing here has.
2. **B5/F17** remove the old agent tools (`suggest_edit`, `edit_document`, `save_document`,
   `set_table_row`, `set_document_fields`, `lookup`, `file_document`). Watched folders and
   the table lookups of company rules still use them.
3. **F16** files above 25 MiB: streamed uploads and streamed copies to the computer.
4. **E13** people and agents in one list in the share dialog.
5. Remove the computer on/off setting; `shared/` is no longer offered to agents but
   still exists for workspaces that keep computers.
6. Shared at 10,000 items is the slowest list (573 ms): it still asks what each row's
   person was given.

## Known gaps in what is built

- Nothing here has run with a real model or a real provider computer yet; the checks
  use the stand-in computer, local file storage and a fixture embedding service.
- Moving offers top-level folders only.
- A file's suggestion (Suggest level) is refused; only pages have suggestions.
