# Skills (schema 50)

How-to packages for agents in the open Agent Skills format
([agentskills.io](https://agentskills.io/specification)), built in M6 from the
[skills plan](../proj-docs/skills-plan.md) (D234). A skill is `SKILL.md` (with a
`name` and a "Use it when…" `description`) plus optional text files.

## Runtime decision (D234 slice 1)

**Option B: skills are served by `ayati_control`.** Tested with Goose **1.54.0**
(`tests/agent_skills.mjs`, real Goose with the scripted provider), 2026-10-31.

- **Why not Goose's Skills extension (option A):** it always includes Goose's
  built-in skills (`SkillsClient::default()` adds `web_search` and
  `goose_doc_guide`, with no setting to hide them), and it reads skill folders
  from the agent's home, which would mean writing files on every stateless
  executor (D225).
- **How B works:**
  - The agent's instructions list `name: description` for each assigned skill,
    only when controller tools are available.
  - `ayati_control__load_skill {name, file?}` returns `SKILL.md` or one supporting
    file from the version current when the request started.
  - Unassigned skills and unknown files return `not_found`.
  - Each load is recorded (`relay_skill_loads`) and shown on the request in
    Activity: "Used skill: quote-builder (version 3)".
- **Limits:**
  - Scripts are stored as text and never run; the tool says so.
  - Loading depends on the model choosing to call `load_skill`. Whether the
    hosted default model picks the right skill from its description is checked in
    the cloud run, not locally (the local provider is scripted).

## Rules

- **Workspace skills** are shared; only admins add, change, share or delete them.
- **My skills:** anyone can add them; the owner (or an admin) changes them; they
  can only go on agents their owner owns. Admins see everyone's.
- **Names are unique in a workspace** and can't change, because agents load
  skills by name. Every change is a new version with a SHA-256 content hash;
  older versions stay readable.
- **On an agent:** its owner or an admin adds or removes skills; at most 20 per
  agent, and their names and descriptions together at most 8,000 characters.
- **Audit log:** adding, changing, sharing, deleting and assigning skills.

## Upload checks

The browser unzips (`.zip`), reads a folder, or takes a pasted `SKILL.md`, and the
server checks the text it receives (`skill_files.rs`), then shows every file for
review before saving:

- at most 100 files and 5 MB; `SKILL.md` at the top, at most 500 lines;
- names follow the spec (1–64 of `a-z`, `0-9` and single hyphens);
- relative paths only: no `/…`, `..`, `.`, backslashes or odd characters;
- text files only (`.md`, `.txt`, `.json`, `.csv`, `.tsv`, `.yaml`) plus script
  sources kept as text;
- the server never decompresses archives and never writes skill files to disk,
  so links can't point outside the skill. The browser checks archive sizes before
  and after unpacking, so a small archive can't expand into something huge.

**Download** gives a standard `.zip` with a folder named after the skill. Uploading
it again gives the same files and the same hash.

**Dependency:** [`fflate`](https://github.com/101arrowz/fflate) 0.8.3 (MIT, no
dependencies) reads and writes zips in the browser. Chosen over a server zip
library so the server never parses untrusted archives.

## Screens

Built from the approved Agents designs (2026-10-30, screens 2 and 8) and the Add
skills panels (`designs/chat-and-account/` screens 7–8, D245):

- **Instructions tab → "Skills it can use":** list with version and uses this
  week, **＋ Add skills**, and ⋯ View and download / Remove.
- **Create step 3 → Skills:** chips under the Ground rules box with **＋ Add
  skills**; they are added right after the agent is created.
- **Add skills** (one drawer): a search across name, description and owner; "In
  this workspace", "Yours" and, for admins, "Other people's", with counts; flat rows
  where **Add** attaches at once; unusable skills stay greyed with the reason as
  text. Footer: "N added to <agent>", **Upload a skill** and **Done**.
- **Upload a skill** (second step, back arrow): choose or drop a `.zip` or folder,
  or paste a SKILL.md; then the checked file, what was read, one file list with
  in-place previews, plain warnings and, for admins, "Share with the workspace".
  **Add skill** saves and adds it; Back and Cancel return to the list unsaved.

**Waiting for designs:** the Skills library page in Agents (lists, history,
create-in-Cuckoo form, request to share), starter skills and "Save a chat as a
skill" (D234 slices 2–3).

## Written in chat

While setting up an agent (create step 3) and on its Instructions tab, the person
describes the job and a model writes the **Ground rules** (the always-on
instructions) and skills as drafts (designs/agent-built-skills 2 and 5).

- **Setting:** `AYATI_SKILL_WRITER_MODEL=provider:model` from the model providers
  file. Without it the chat is hidden and both are written by hand ("Write a
  skill" and the Ground rules box).
- **What the model gets:** the fixed guide (`skill_writer/guide.rs`, shown in the
  chat as "What … is told while setting up"), then the setup as JSON data: the
  agent's name and job title, its apps with each action's title and kind (the
  connectors chosen in step 2, or a saved agent's switched-on ones), workspace
  skills and the person's own for reuse, the ground rules and current skills, and
  the conversation (at most 40 messages). No tools, no business data, no keys.
- **What comes back** (`POST …/skill-writer`): a reply, new ground rules or none,
  new or changed skills, names to remove or reuse, apps it needed but the agent
  lacks, and notes. Every skill becomes a SKILL.md that passes the upload checks;
  names are made valid and, when taken anywhere in the workspace, get the agent's
  name added (`refund-check-iris`), with a note. Nothing is saved by this route.
- **Saving:** in New agent, drafts become the owner's own skills and are attached
  on Create agent, each with a key that changes with its text, so a retry saves
  one skill. On the agent page each proposal is shown as a line diff and applied
  on its own: ground rules go into the box (saved with Save changes); a changed
  skill is saved as its next version keeping its other files; new skills are
  created and attached; removals and reuse use the existing assign route.
- **Who:** anyone creating an agent; on a saved agent, its owner or an admin (the
  route checks it). Each turn is recorded as model usage (`skill_writer`).
- **Limits:** the model only drafts; quality depends on it. A refused or failed
  turn keeps what was typed.

## API

Under `/api/identity/workspaces/{w}`: `GET/POST /skills`, `POST /skills/check`,
`GET /skills/{id}?version=`, `POST /skills/{id}/versions`, `POST /skills/{id}/share`,
`POST /skills/{id}/delete`, `GET/POST /agent-profiles/{agent}/skills`. Uploads
accept up to 8 MB of JSON.

## Checks

- PostgreSQL: `skills_tests` (permissions, versions and retries, assignment rules
  and budget, loads at the starting version, Activity steps) and the file checks'
  unit tests (names, paths, types, size, the expanded-archive limit).
- Guest: `load_skill` forwards only a name and an optional file.
- Web: `tests/agent-skills.test.mjs` (zip round trip, archive limits, dropped
  folders, search, Add once, reasons as text, the upload step, Back/Cancel, failed
  saves and retries, keyboard file control, focus on close, remove) and
  `tests/agent-create.test.mjs` (step 3 Skills).
- Journey: `tests/agent_skills.mjs` with real Goose 1.54.0.
