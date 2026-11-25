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
| B4 | Open on the computer and Save back | Next |
| B5 | Five agent tools instead of ten | Not started |
| B6 | A computer for every agent | Not started |
| C7 | Each agent's own folder | Not started |
| C8 | Chat attachments as Library items | Not started |
| D9 | Exact search mode | Not started |
| D10 | Meaning search per folder, off by default | Not started |
| D11 | Descriptions instead of automatic summaries | Not started |
| D12 | Unanswered questions counted per question | Not started |
| E13 | One share dialog for people and agents | Not started |
| E14 | Folder tree, "Made by agents", working note | Not started |
| F15 | Keeping rule on agents' folders | Not started |
| F16 | Files above 25 MiB | Not started |
| F17 | Remove the old tools and paths | Not started |
