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

Built from the approved Agents designs (2026-10-30, screens 2 and 8):

- **Instructions tab → "Skills it can use":** list with version and uses this
  week, **＋ Add skill**, and ⋯ View and download / Remove.
- **Create step 2 → Skills:** chips with **＋ Add skills**; they are added right
  after the agent is created.
- **Add skills** (drawer): workspace skills, my skills (and others' for admins),
  and **Upload a new skill** with review and, for admins, "Share with the
  workspace".

**Waiting for designs:** the Skills library page in Agents (lists, history,
create-in-Cuckoo form, request to share), starter skills and "Save a chat as a
skill" (D234 slices 2–3).

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
- Web: `tests/agent-skills.test.mjs` (zip round trip, archive limits, picker,
  upload review, double-click save, remove).
- Journey: `tests/agent_skills.mjs` with real Goose 1.54.0.
