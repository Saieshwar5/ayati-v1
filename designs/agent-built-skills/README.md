# Skills written by the agent, and a visible system prompt (proposal)

Waiting for the owner's approval; nothing is built. Replaces the earlier
"instructions and skills in one place" proposal (same branch, previous commit).
It changes step 2 of New agent and the Instructions tab from the approved
[Agents](../agents-simplified/README.md) designs (screens 2 and 8) and builds on
skills (D234) and the Add skills panels from [chat and account](../chat-and-account/README.md).
Editable source: [mockups.html](mockups.html), opened in a browser from this
folder. The example company is fictional.

## The idea (owner, 2026-11-08)

- Every agent gets the same **Cuckoo system prompt**. Nobody in a workspace can
  edit it, and everyone who can see an agent can read all of it, during creation
  and afterwards.
- Everything specific to an agent is a **skill**. During creation the person
  describes the agent in their own words; the agent asks what it needs and
  writes its skills. The person reviews them, edits any skill by hand or asks the
  agent in chat to change it. An agent can have several skills.
- Skills made anywhere else (for example in Claude) can be **uploaded** too, as
  today: a `.zip`, a folder or a `SKILL.md`.
- **Order:** Name and picture → Skills → Tools and safety → People. Tools come
  after skills so they can be suggested from what the agent will do.

## Additions to keep it safe

- **Ground rules, always on.** A skill's steps reach the model only when it calls
  `load_skill`, so restrictions in an ordinary skill are optional. The agent
  puts identity, tone, "never" and "ask a person when" into one Ground rules
  skill that is read with every message. It is stored in today's
  `settings.instructions`, so existing agents' instructions become their Ground
  rules with no data migration.
- **Enforce what can be enforced.** A restriction that matches an action
  ("never give discounts") is suggested as a Block or Ask first rule in Tools and
  safety. Skills are guidance; rules are enforced.
- **Setup mode has no tools.** While writing or changing skills the agent has no
  tools, no business data and no executor; it can only return drafts. Nothing is
  saved until the person saves it.
- **Only the owner, managers and admins change skills,** in the private Change
  with Iris chat. In ordinary Chat an agent never changes its own skills, so a
  customer can't rewrite its rules.

| File | Screen and notes |
| --- | --- |
| `1-teach-the-agent.png` | Create step 2, now "Skills". (1) The Cuckoo system prompt, locked, with Read all of it. (2) Describe the agent by text or voice; it asks a few questions. (3) Drafts: Ground rules (always on) and task skills. (4) Upload a skill made elsewhere; the agent reviews it and says what won't work here. (5) Workspace skills reused instead of copied. (6) Edit, Ask Iris, Remove; nothing saved until Create agent; next step suggests tools. |
| `2-change-a-skill-with-chat.png` | Agent page, Skills tab (was Instructions). (1) Ground rules first, then skills with version and use. (2) The proposed change as a diff. (3) Change with Iris: private setup chat that proposes, never saves. (4) Save as version 3 or Discard; versions record that the change came from chat. (5) Edit by hand. |
| `3-everything-it-receives.png` | Everything Iris receives. (1) The full Cuckoo system prompt, dated. (2) Each section says whether it is included for this agent and why. (3) Ground rules and the skill list. (4) What is added per message and only when needed. (5) Copy as text. |
| `4-tools-from-skills.png` | Create step 3, Tools and safety. (1) Suggested from the skills; Clear suggestions. (2) Each connector names the skill that needs it; unneeded ones stay off. (3) Restrictions become Ask first or Block, quoting the person's words. (4) Admin limits still win. (5) A warning when a skill needs something that isn't connected. People becomes step 4 with Create agent. |

## Skills from outside

- Upload accepts what it does today (`skill_files.rs`): a `.zip`, a folder or a
  pasted `SKILL.md`; at most 100 text files and 5 MB, `SKILL.md` at most 500
  lines, names in the open Agent Skills format. Skills from Claude and other
  tools use the same format.
- Scripts are kept as text and never run, so a skill that depends on its script
  only partly works. The review says so.
- Uploaded text is untrusted. The agent reviews it in setup mode and points out
  instructions that clash with Ground rules, ask for tools the agent doesn't
  have, or try to send data elsewhere. A skill never changes tools, rules or
  approvals.
- An uploaded skill is edited like any other: by hand, by chat, or by uploading
  a newer file with the same name as its next version (new: "Upload a new
  version" on the skill). Download .zip gives the standard format back.

## Why skills come before tools

- People think about the job first; the tools follow from it.
- Tools can then be suggested with a reason (the skill that needs them), which
  keeps an agent to the tools it uses. Today every connector starts off and the
  person has to guess.
- Restrictions from the description become enforced rules in the same place.
- The agent still knows what exists: while writing skills it sees the names of
  connectors and actions the admin added (no access), so skills don't promise
  actions that aren't available.
- Nothing is live until Create agent, so this order doesn't weaken safety.

## How it would be built

- **Builder:** a controller-side model call per turn, like the Library's summary
  calls (`src/identity/library/models.rs`), with a fixed builder prompt that is
  shown with the system prompt. It receives the conversation, the current drafts
  and the workspace's skill names, and returns a reply plus structured drafts
  (name, "Use it when", steps, optional files, always on or not). Drafts pass the
  same checks as uploads (`skill_files.rs`) before they are shown.
- **Storage:** Ground rules in `settings.instructions`; other skills as today's
  skills, assigned to the agent. Create, check, new version and assign already
  exist on the server.
- **System prompt view:** move the fixed paragraphs now appended in
  `src/identity/chat/execution.rs` into one function used by both real requests
  and the preview, returning named sections with whether each is included. A test
  checks the preview against a real request.
- **Skills without controller tools:** today the skill list is sent only when
  the agent can reach controller tools (business tools or a pooled executor).
  If skills become the main way to shape an agent, `load_skill` must be
  available to every agent, or the agent page must say its skills are unused.

## Behaviour changes to confirm

- Tools and safety moves before People, and is filled in from the skills;
  writes are never suggested looser than Ask first.
- Step 2 and the Instructions tab are renamed **Skills**; the free-text
  Instructions box is replaced by the Ground rules skill.
- Skills written by the builder belong to the agent's owner and are used only by
  that agent unless shared.
- Each builder turn is a model call on the workspace's default model.
- "Try a sample task" was removed earlier (D233); a check of which skill the
  agent would open for a message could return later if wanted.
