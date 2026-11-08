# Agent setup: connectors, skills written by the agent, and controls (proposal)

Approved by the owner on 2026-11-08. Built so far: the visible system prompt
(screen 6) and the five-step flow with today's controls (screens 1, 3 and 4, with
the Ground rules box and Add skills in step 3). Not built yet: skills written in
chat (screens 2 and 5) and suggested controls. Replaces the earlier
"instructions and skills in one place" proposal (same branch). It changes the
create flow and the Instructions tab from the approved
[Agents](../agents-simplified/README.md) designs and builds on skills (D234),
company rules ([guard rules](../../runtime/guard-rules.md)) and the Add skills
panels from [chat and account](../chat-and-account/README.md). Editable source:
[mockups.html](mockups.html), opened in a browser from this folder. The example
company is fictional.

## The idea (owner, 2026-11-08)

- Every agent gets the same **Cuckoo system prompt**. Nobody in a workspace can
  edit it, and anyone who can see an agent can read all of it.
- Everything specific to an agent is a **skill**. The agent writes its skills
  from the person's own description; the person edits them by hand or asks the
  agent in chat. Skills made elsewhere (for example in Claude) can be uploaded.
- **Create order:** Name and picture → **Connectors** → **Skills** → **People** →
  **Controls**. The agent knows its apps before writing skills, and suggests the
  controls last, from the apps, skills, what was said and who can talk to it.

| File | Screen and notes |
| --- | --- |
| `1-connectors.png` | Step 2, new. (1) Which apps the agent can use; only apps and MCP servers an admin added. (2) What each can do, by kind: reads, changes, money, sends outside, deletes. (3) On means available, not allowed. (4) A missing app is requested from an admin. (5) The setup explained once; skipping is fine. |
| `2-skills.png` | Step 3. (1) The locked Cuckoo system prompt. (2) The agent knows its apps and asks only about the job, by text or voice. (3) Ground rules, always on. (4) Each skill shows which app actions it uses (information, not permission). (5) Upload a skill made elsewhere; the agent checks it. (6) A skill needing a missing app asks to add it. |
| `3-people.png` | Step 4, the approved People step moved before Controls. (1) Who can talk to the agent. (2) Managers approve Ask first. (3) Both feed the Controls suggestions. |
| `4-controls.png` | Step 5, last. (1) Suggested from apps, skills, what was said and people; Reset. (2) Kind and reason per action. (3) Unused actions blocked. (4) Only stricter suggestions; Allow is the person's choice, with their limit kept as a condition. (5) Admin limits win. (6) Create agent; Cuckoo enforces before every action. |
| `5-change-a-skill-with-chat.png` | Agent page, Skills tab (was Instructions). Ground rules first; a change from the private Change with Iris chat shown as a diff; Save as the next version or Discard; Edit by hand. |
| `6-everything-it-receives.png` | The full Cuckoo system prompt in sections, each marked included or not and why; then Ground rules, the skill list, per-message context and on-demand skill steps. |

Step 1, Name and picture, is unchanged.

## Rules that keep it safe

- **Ground rules, always on.** A skill's steps reach the model only when it calls
  `load_skill`, so "never" and "ask a person when" live in one Ground rules skill
  read with every message. It is today's `settings.instructions`: no migration.
- **Setup mode has no tools.** While writing or changing skills the agent has no
  tools, no business data and no executor. It returns drafts; nothing is saved
  until the person saves.
- **Controls only get stricter by suggestion.** Suggestions never go looser than
  Cuckoo's safe defaults (read Allow, delete Block, everything else Ask first).
  Allow on a change, money or sending action is the person's choice. Admin limits
  and company rules still win. Cuckoo enforces controls before every action; the
  agent can't change them while working.
- **Skills never grant access.** "Uses Zoho Books: Create a credit note" is
  information for Controls, unlike Claude's `allowed-tools`.
- **Uploaded skills are untrusted text.** Scripts are stored and never run. The
  agent points out steps that clash with Ground rules, need missing apps or send
  data elsewhere.
- **Only the owner, managers and admins change skills,** in the private chat on
  the agent page. In ordinary Chat an agent never edits its own skills.

## How it would be built

- **System prompt view:** move the fixed paragraphs appended in
  `src/identity/chat/execution.rs` into one function used by real requests and the
  preview, returning named sections with whether each is included. Today the
  preview shows only `SYSTEM_GUIDE`, one of about 20 paragraphs. A test checks the
  preview against a real request.
- **Builder:** a controller-side model call per turn, like the Library summaries
  (`src/identity/library/models.rs`), with a fixed builder prompt shown with the
  system prompt. Input: the conversation, drafts, the chosen apps' action names and
  kinds, and the workspace's skill names. Output: a reply and structured drafts
  that pass the upload checks (`skill_files.rs`).
- **Storage:** Ground rules in `settings.instructions`; other skills as today's
  skills, assigned to the agent, plus the actions each uses.
- **Controls:** per-action Allow / Ask first / Block for an agent already exists
  (stricter than the workspace). New: conditions for one agent, using the company
  rules engine (amount, daily total, time), stricter only, worded by code
  (`guard::summary`), not by the model. Today conditions are workspace-wide and
  admin-only.
- **Skills on every agent:** today the skill list is sent only when the agent can
  reach controller tools (business tools or a pooled executor). Skills need to work
  for every agent, or the agent page must say they are unused.
- **Later reviews:** when a skill or app changes, the Tools tab flags rules that no
  longer fit ("Refund check now uses Create a credit note, which is blocked").

## Behaviour changes to confirm

- Five steps instead of four; Instructions becomes Skills; Tools and safety splits
  into Connectors (step 2) and Controls (step 5); Create agent moves to Controls.
- Suggested controls arrive filled in, marked with their reason, with Reset to
  suggestions. Today every connector starts off with the admin's modes.
- Skills written by the agent stay on that agent unless shared.
- Each builder turn is a model call on the workspace's default model.
