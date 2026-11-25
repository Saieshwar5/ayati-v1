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
| B6 | A computer for every agent | Not started |
| C7 | Each agent's own folder (schema 155) | Built and tested; made when the agent first saves a file |
| C8 | Chat attachments as Library items | Not started |
| D9 | Exact search mode | Built and tested (`exact: true`); folder, kind and date filters not yet |
| D10 | Meaning search per folder, off by default (schema 156) | Built and tested; agents propose, the manager decides |
| D11 | Descriptions instead of automatic summaries | Partly: automatic summaries now only in meaning-search folders; the description line is not built |
| D12 | Unanswered questions counted per question | Not started |
| E13 | One share dialog for people and agents | Not started |
| E14 | Folder tree, "Made by agents", working note | Not started |
| F15 | Keeping rule on agents' folders | Not started |
| F16 | Files above 25 MiB | Not started |
| F17 | Remove the old tools and paths | Not started |

## Next, in order

1. **C8** chat attachments stored as Library items in the conversation's subfolder (the
   chat and Library file stores are separate today, so "stored once" needs one store).
2. **B6** computers on for every agent by default.
3. **E13/E14** one share dialog for people and agents; an agent's folder on its page;
   "Made by agents"; the working note.
4. **D11/D12** the description line; unanswered questions counted per question.
5. **B5/F17** remove the old tools (`suggest_edit`, `edit_document`, `save_document`,
   `set_table_row`, `set_document_fields`, `lookup`, `file_document`) once the new ones
   have run with a real model; the second receipt table; orphan take-over screens.
6. **F15/F16** a keeping rule on agents' folders; files above 25 MiB; the 1,000-item
   limit, which agents' folders will reach sooner.

## Known gaps in what is built

- Nothing here has run with a real model or a real provider computer yet; the checks
  use the stand-in computer, local file storage and a fixture embedding service.
- Moving offers top-level folders only.
- A file's suggestion (Suggest level) is refused; only pages have suggestions.
