# Agents that get better on their own (schema 143)

Built 2026-11-24 from [the proposal](../designs/agent-evolution/README.md). Every
answer gets an outcome; memory learns from outcomes; new notes are checked; requests
are sorted into each agent's kinds of work; and every three days each agent reviews
its routine work and proposes skills, which one of its owners or an admin approves.
A skill an agent wrote says **Created by <agent>** wherever it is shown.

## Settings

| Setting | What it turns on |
| --- | --- |
| none | Outcomes from plain signals: a refused approval is a correction, a person's reaction (👍 ✅ 🙏 👏 🎉 💯 ❤️ 🔥) with no reply is an acceptance, another reaction is a follow-up, six hours of silence is "not known". |
| `AYATI_CLASSIFIER_MODEL=provider:model` | The classifier: reading the asker's reply, holding the notes a correction contradicts, checking new notes, sorting requests into job types. |
| `AYATI_LEARNING_MODEL` as well | Three-day skill reviews: drafting and rehearsing skills. |
| `AYATI_LIBRARY_EMBEDDING_MODEL` and pgvector | Notes' meanings: `search_memory` also finds notes close in meaning that share no word with the question. |

All use `AYATI_MODEL_PROVIDERS_FILE` and `/chat/completions` (embeddings
`/embeddings`), as the other controller models do. Usage is recorded against the
agent as assignments `classify` and `skill_review`, with no catalog charge, and a
review is skipped when the agent's model budget is used up or agents are paused.

## The classifier

One small model answering fixed questions (`src/identity/evolution/classify.rs`).
Each answer is JSON with a `confidence`; only listed values are read, anything else
is dropped. At or above 0.8 the answer acts; from 0.5 a disagreement waits for a
person; below 0.5 it is ignored. The data it reads is labelled information, never
instructions.

| Question | Acts on |
| --- | --- |
| How did the person take the answer? (accepted, corrected, followed up, unrelated) | the request's outcome |
| Which of these notes does the correction show wrong? | those notes go on hold |
| Kind, lasting, private, an instruction; same, contradicts or unrelated to each nearest note | the new note |
| Which job type is this request, or a new one? | the request's job type |
| Does a skill already cover this work? (yes, partly, no) | whether a review drafts |
| Did the rehearsal do as well as the real answer? (better, same, worse) | whether a draft is kept |

## Outcomes, and memory learning from them

A controller task (`src/identity/evolution/mod.rs`) runs every ten minutes over
finished answers of the last two weeks with no outcome. The asker's next message in
the same thread within six hours is read by the classifier; plain signals decide
otherwise (above). Activity shows **Outcome: accepted / corrected by <person> /
followed up / not known**.

Every note a request was given or found is recorded (`relay_memory_used`). When the
outcome is confident:

- **accepted:** each note +5 strength and its fading clock restarts;
- **corrected:** each note −10, and the notes the classifier says the correction shows
  wrong go **on hold**: waiting under Needs your answer with the reason "An answer
  that used it was corrected", not given or found until a person keeps or removes it.

Since schema 143 a search that finds a note only restarts its clock; strength follows
outcomes.

## Checking new notes

Each note an agent keeps is checked once (`checked_ms`, `check_result`) against the 5
notes on its shelf sharing most words with it:

| The classifier says | The note |
| --- | --- |
| an instruction | ended |
| one-off | ended |
| private, on the job or workspace shelf | waits for a person (`private_source`) |
| another kind | its kind is corrected |
| the same as a nearby note | ended; the older note +15 |
| contradicts a nearby note not much stronger (≤ +10) and not pinned | the older note ends, replaced by it |
| contradicts, otherwise | waits as a disagreement, showing both |

Notes people wrote are not checked; notes kept before schema 143 count as checked.

## Job types

Each finished request with an outcome (not an assistant's) is sorted into one of its
agent's job types, or a new one named by the classifier ("Send a payment reminder");
a name is unique per agent ignoring case, and a job type is never another agent's.
The agent's **Improving** panel lists its kinds of work in the last 30 days with
accepted and corrected counts.

## Skill reviews every three days

An agent is due when it is managed (not an assistant), learning is on (the lessons
switch), its last review was at least 72 hours ago and at least 10 requests were
sorted since. A round reviews at most three agents. The review
(`src/identity/evolution/review.rs`) reads the last 30 days:

1. **Routine work:** job types done at least 5 times on at least 3 different days
   (up to 5), each with up to 6 examples, corrected ones first.
2. **Skip** job types with an open proposal or one rejected in the last 30 days.
3. **Coverage:** with no skills, none covers it; otherwise the classifier answers.
4. **Decide:** no skill → draft a **new skill**; a skill covers it but a third or more
   of its runs were corrected → draft a **new version**; otherwise nothing.
5. **Draft** with the learning model from the examples: the tool steps of every run,
   but request and answer text only from conversations two or more people read
   (a private chat shows as "(a private conversation)").
6. **Check:** a valid SKILL.md (the upload checks), no scripts, no passwords, keys or
   card numbers, and no member's name, email address, run of five digits or amount
   (the lessons filter). A failing draft is dropped.
7. **Rehearse** on up to three of the examples: the learning model writes the reply
   it would send following the draft, with no tools; the classifier compares it with
   the real answer (and its correction). Any confident "worse" drops the draft.
   Rehearsal notes are kept only for shared conversations.
8. **Retire:** one skill on the agent nobody loaded for 60 days, created 60 days ago
   or more, may be proposed for retiring.

At most two proposals a review. Every review is saved with one line, also when it
proposes nothing ("Checked 41 requests in 6 job types: no new skill needed (…)").

## Approving

Agents › the agent › Knows › **Improving**, for its owners and admins:

- this week against last: answers, corrected, accepted and followed up per 100, memory
  searches that found nothing, notes learned and held, skills approved; each change
  said in words;
- **skills it proposes**: why, the requests it came from (their words only from
  conversations the viewer can open), the rehearsal, the SKILL.md; **Approve**,
  **Edit** (then Approve edited skill) or **Reject**;
- every review, the skills it created with corrections before and since, its kinds of
  work, and decided proposals.

**Approve** a new skill saves it as an owner's own skill (the approver when they own
the agent, otherwise its first owner), with `created_by_agent`, version 1 written by
the agent and approved by the person, and puts it on the agent (within its 20 skills
and listing size). A new version is written by the agent and approved by a person
who may change the skill. Retiring takes the skill off the agent. A repeated decision
changes nothing; the opposite decision is refused. Audit lines name the agent.
**Reject** keeps nothing; that work is not proposed again for 30 days.

Nothing an agent writes is used before a person approves it. A skill grants no
permission: apps, approvals and company rules still decide what an agent may do.

## Where it shows

- Skill drawer: **Created by <agent>**, and versions "written by <agent>, approved
  by <person>". Agent page skill chips: "by <agent>". Skills lists carry
  `created_by_agent`.
- Activity: "Used skill: payment-reminder (version 2, created by Nova)" and the outcome.
- Settings › Memory: **How your agents are learning**, the workspace's week and each
  agent's corrected answers against last week; held notes under Needs your answer.

## API

- `GET …/agent-profiles/{a}/improving` → `{available, this_week, last_week, reviews,
  jobs, skills, proposals, configured}` (owners and admins).
- `POST …/agent-profiles/{a}/skill-proposals/{p}` with `{approve, files?}`.
- `GET …/workspaces/{w}/learning` → `{available, this_week, last_week, agents,
  configured}` (admins).

Tables: `relay_request_outcomes`, `relay_memory_used`, `relay_job_types`,
`relay_request_job_types`, `relay_skill_reviews`, `relay_skill_proposals`,
`relay_memory_embeddings` (with pgvector); new columns `relay_memories.checked_ms`
and `check_result`, `relay_skills.created_by_agent`, `relay_skill_versions.agent`;
the waiting reason `corrected`. Apply `m20261124_000143_evolution` with the
administrator migration procedure; it adds tables and columns only.

## Checks

- PostgreSQL `evolution::tests`: outcomes from reactions, silence and no classifier;
  an outcome saved once; accepted and corrected changing used notes; holding only the
  request's notes; the Activity line; note checks merging, waiting on a much stronger
  note, replacing a weaker one, ending an instruction, holding a private note, once;
  job types per agent; Improving for owners and admins only; the workspace week.
- `evolution::review_tests` with a fake provider: the classifier reading a reply and
  holding the contradicted note; 13 classifier calls recorded; job types; a first
  review with nothing routine; routine work on four days proposing a skill with three
  rehearsals and evidence; members refused; a double approval making one skill,
  created by the agent, on the agent, with its version's author; no review within 72
  hours; drafts naming a member or rehearsing worse dropped; a rejection waiting.
- Unit: answers read only as allowed values, confidence clamped; skill names; config.
- Web `tests/improving.test.mjs`: one decision for a double click, evidence and
  rehearsal, an edited draft, rejection, a refused member, the workspace week; the
  agent page lists the Improving chip.

**Not run with a real model**: whether the classifier judges answers well, the
groupings make sense and drafted skills help is unproven; choose and check the
models on labelled examples first.

## Limits and not built

- Only **Ask first**: there is no setting to use a skill after it passes rehearsal.
- Rehearsal uses the controller's learning model, not the agent's own executor.
- A draft is told to use only the agent's apps, but that is not checked.
- Nearest notes are found by words; duplicates by meaning wait for embeddings of the
  new note at check time.
- Not built: members of a room seeing its notes, grouping "looked for, not found" into
  themes, moving assistant memory onto the engine, "the same question asked again"
  and "steps per job" measures, a count of proposals on the Improving chip.
