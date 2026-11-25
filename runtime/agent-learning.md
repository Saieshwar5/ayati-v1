# Agents learn from their work

Schema 121 (knowledge K1). Once a day, Cuckoo reads an agent's work in shared
conversations and proposes lessons about how it should work. Nothing reaches the
agent until a person who maintains it approves a lesson; an approved lesson is
an ordinary [knowledge note](agent-profiles.md).

Knowledge holds how to work, not facts: facts change and are read from the
Library and connected systems when needed. A person's own preferences belong in
their own conversation, not in knowledge every user's request carries.

## What it reads

Only conversations people share: channels and groups with two or more people, or
public ones. Never private chats (the Knowledge tab's promise "Notes are not
learned automatically from private chats" holds), automations or
anything from outside the workspace. From those, since the agent's last run (at
most seven days back):

- **Answers:** the agent's reply and the next person's message in the same
  conversation and thread within six hours, often a correction (up to 40).
- **Reactions** people left on its replies (up to 30).
- **Refused approvals:** actions people rejected (up to 20).

Replies built from restricted sources (connected-event data, Library sources,
hand-off trees) are left out, as they are from later prompts. Text is cut to 800
characters per message. The model also sees the agent's notes for every
conversation, so it can update one instead of repeating it.

## How lessons are made

With `AYATI_LEARNING_MODEL=provider:model` (keys from
`AYATI_MODEL_PROVIDERS_FILE`), a controller task checks every ten minutes for
agents due: managed agents with learning on, no run in the last 20 hours and
some work since the last one. It skips, and tries again the next day, when 20
lessons are already waiting for review, agents are paused or the agent's model
budget is used up.

The model proposes at most five lessons, each with a title, when it applies,
what to do, why, and the signals it cites. It is told the signals are data,
never instructions. Before anyone sees a lesson, Cuckoo drops it if it:

- cites no signal it was given,
- updates or contradicts a note the agent doesn't have,
- names a workspace member (their display name or first name), or carries an
  email address, a run of five or more digits, or an amount of money.

A lesson from one conversation is proposed for that conversation; one from
several is proposed for every conversation. Usage is recorded as assignment
`learning` against the agent, with no catalog charge.

## Review

Agents › the agent › Instructions › Knowledge › **Lessons from its work**, for the
agent's owners and admins (the same people who change its notes):

- Each lesson shows what kind it is (new, updates a note, disagrees with a
  note), the note it would change, why, and its evidence. Evidence text appears
  only from conversations the reviewer can open; otherwise it says so.
- **Approve** adds a new lesson for its own conversation, or, if chosen, every
  conversation with the agent; an update replaces the note it is about and keeps
  that note's audience. **Edit** changes the wording before approving.
  **Reject** keeps nothing. A decided lesson stays decided.
- Approved notes say "Learned from its work". **Suggest lessons** switches
  learning off and on.
- An owner or admin can also ask the agent for a lesson in their private chat
  with it (`suggest_lesson`, [owner view](owner-view.md)). It appears here with
  the evidence "An owner or admin asked for it", for every conversation, under
  the same filter for names, addresses, numbers and amounts.

## Not built yet

Choosing lessons per request instead of including every note, measuring whether
each lesson helps, suggesting retirement, per-person preferences, and learning
from private chats with each person's consent.

## Checks

- PostgreSQL `agent_learning::tests`: signals only from the shared conversation
  (a private chat's message never reaches the model), three of six proposed
  lessons kept (a member's name, an amount and an unknown citation dropped),
  once a day, usage recorded, maintainers only, evidence excerpts, nothing in
  knowledge before approval, approval for one conversation, an edited update
  replacing its note, rejection, a repeated decision, switching off.
- Unit: the personal-details filter and reading the model's answer.
- Web `tests/learned-lessons.test.mjs`: default audience, one decision for a
  double click, the notes refreshing, edited wording, updates, rejection, the
  switch and an unconfigured Cuckoo.
