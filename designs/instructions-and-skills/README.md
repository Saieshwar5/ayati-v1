# Instructions and skills in one place (proposal)

Waiting for the owner's approval; nothing is built. It changes step 2 of New agent
and the Instructions tab from the approved [Agents](../agents-simplified/README.md)
designs (screens 2 and 8) and the Add skills panels from
[chat and account](../chat-and-account/README.md) (screens 7–8, D234, D245).
Editable source: [mockups.html](mockups.html) (open it in a browser from this
folder; there is no OpenPencil file yet). The example company is fictional.

## The problem

An agent gets two separate ways to receive guidance, and they look unrelated:
a large Instructions box and, under it, a strip of Skills chips that opens a
library. Creating a skill needs a `.zip`, a folder or a pasted `SKILL.md` with
YAML front matter, and a skill can't be edited in Cuckoo (only viewed,
downloaded and removed), although the server already saves new versions
(`POST /skills/{id}/versions`).

## Why the instructions box stays

Skills only reach the model as `name: description` lines. Their steps are read
only if the model chooses to call `load_skill`, and the listing is sent only when
controller tools are available (`chat/execution.rs`). Rules such as "Never
promise refunds" or "Ask a person when…" must be read on every request, so they
can't live only in a skill. Removing the box would make hard rules optional.

The proposal keeps both mechanisms but presents them as one step with two
layers, and makes writing and editing a skill as easy as typing instructions.

| File | Screen and notes |
| --- | --- |
| `1-create-instructions.png` | Create, step 2. (1) Always follow: today's box, for identity, tone, hard rules and when to ask a person. (2) Skills as a list in the same step, each with its "Use it when" line. (3) Write a skill (main) and Add from library; import moves inside the library. (4) Starters also suggest skills; added only with Add. (5) "How Iris uses this" explains what goes where. |
| `2-write-a-skill.png` | Write a skill, in the same panel as Add skills. (1) Opens over step 2 and on the Instructions tab. (2) A normal name; Cuckoo shows the saved name and checks it is free. (3) "Use it when" and Steps; Write it for me is optional. (4) Text files the steps can point to. (5) Import a skill file keeps today's upload. (6) Save and add saves to Yours at once and adds it to the agent when it is created. |
| `3-edit-a-skill.png` | Instructions tab with Edit open. (1) Edit for the owner of a personal skill or an admin for a workspace skill; the name stays fixed. (2) Which agents a save affects. (3) Changed lines counted. (4) Files edited, replaced or removed. (5) Save as the next version; History; a newer save asks to reload. |
| `4-states.png` | (1) A member reads a workspace skill: Make my own copy, Download .zip. (2) Copy with a new name, optionally replacing it on the agent. (3) Name taken (offer the existing skill) and a failed save that keeps every field. (4) Later: "Move to a skill" when Always follow grows step-by-step sections. |
| `5-phone.png` | Step 2 stacked with full-width buttons; Write a skill as a full-screen sheet. |

## Behind the scenes

- Cuckoo writes a standard `SKILL.md` (`name`, `description`, the steps as the
  body) plus the extra files, so Download .zip keeps working in other tools.
- The saved name is the typed name in lowercase with hyphens (`Refund check` →
  `refund-check`); lists show it humanized. No new database field is needed.
- Create, check, new version, share, delete and assign already exist on the
  server. New work is client-side, plus listing the agent's skills in "See
  everything it receives".

## Behaviour changes to confirm

- **Words:** the box is renamed "Always follow"; "Upload a skill" becomes
  "Import a skill file" inside Add from library.
- **Saving:** a skill written during New agent is saved to Yours when its panel
  is saved, before the agent exists (as uploads are today), so Cancel on the
  agent keeps the skill.
- **Editing shared skills:** an edit applies to every agent using the skill;
  members copy a workspace skill instead of changing it.
- **Starter skills** and **Move to a skill** are later phases.
