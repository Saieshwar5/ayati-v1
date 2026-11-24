# Agents that get better on their own (proposal, 2026-11-24)

**Status: proposal, not built.** Open [`mockups.html`](mockups.html) for the
screens and [`implementation-plan.md`](implementation-plan.md) for the build steps.
Builds on [the memory system](../memory-system/README.md) (built, schema 142),
[agent learning](../../runtime/agent-learning.md), [skills](../../runtime/skills.md)
and the draft-only [job rehearsal](../../runtime/agent-job-rehearsal.md) runner.

## The goal

A startup or agency using Cuckoo should find its agents better every week than
the week before, without anyone having to teach them by hand. Agents should:

1. **remember** what they learn (built: the memory system);
2. **notice** how each answer landed (new: outcome signals);
3. **turn routine work into skills** of their own, checked every three days (new);
4. **show** that they improved, with numbers (new: the learning score).

People stay in charge: everything an agent creates says so, and nothing new that
changes how an agent works is used before its owner or an admin approves it
(see the one open decision below).

## The loop

```
work ─► signals ─► learn ─► check ─► review ─► use ─► measure ─┐
  ▲                                                             │
  └─────────────────────────────────────────────────────────────┘
```

| Step | What happens | Today | New |
| --- | --- | --- | --- |
| Work | Agents answer requests and run tools | yes | |
| Signals | How each answer landed | reactions, refused approvals | **every answer gets an outcome**: accepted, corrected, followed up, ignored |
| Learn | Kept as notes, lessons and skills | memory notes, daily lessons | **job types** and **skills drafted from routine work** every 3 days |
| Check | Is it safe and right | word rules | **a classifier** for kind, privacy, disguised instructions, duplicates, contradictions; **rehearsal** of a skill on past requests |
| Review | A person decides | waiting notes, lesson review | the same lists, plus **proposed skills** with evidence |
| Use | Given and found while working | notes by words | notes **by meaning**, ranked for the request; skills loaded by the agent |
| Measure | Did it help | Activity lines | **learning score** per agent and workspace; each note's and skill's effect |

## 1. Classification: one small model for many small decisions

The main agent model is expensive and decides differently each time. Many
learning decisions are the same small question asked thousands of times. A fast
classifier answers them with a typed answer and a confidence, and code acts on
the confidence: act above a high line, ask a person in the middle, drop below.

**One classifier service** (`classify`) in the controller, set with
`AYATI_CLASSIFIER_MODEL=provider:model` like the other model settings. Every
question has a fixed set of answers; results are stored with the model, the
confidence and the time, so they can be checked and re-run. Without the setting,
today's rules keep working.

| Question | Asked about | Answers | Used for |
| --- | --- | --- | --- |
| What kind of note is this? | a new note | the 8 kinds | correcting the agent's choice |
| Will this still be true in a month? | a new note | yes / no + confidence | dropping one-off notes |
| Is this about one person's private matters? | a new note, a skill draft | yes / no | replacing the name and number check |
| Is this an instruction, not a fact? | a new note | yes / no | refusing planted orders |
| Same, contradicting or unrelated? | a new note and its nearest notes | three choices | merging, ending, asking an admin |
| Does this note help answer this request? | a request and candidate notes | score | choosing what a request is given |
| How did the person take the answer? | the reply and the next message | accepted, corrected, follow-up, new topic | outcome signals |
| Which job is this request? | a finished request | the agent's job types, or new | job types for skills |
| Does this skill cover this job? | a skill and a job type | yes / partly / no | finding work with no skill |
| Did the rehearsal do as well as the real run? | two answers to one request | better / same / worse | testing a skill draft |
| What are these failed searches about? | "looked for, not found" | short theme | grouping for admins |

**Embeddings** (the Library's `AYATI_LIBRARY_EMBEDDING_MODEL`, with pgvector) give
"meaning" to notes, requests and skills: nearest notes for duplicates, similar
requests for job types, meaning search. Without pgvector, word search stays.

**Choosing the model** is a measurement, not a guess: a labelled set of about 300
real examples per question, and each candidate scored on accuracy, calibration,
speed and cost. A fast general model with structured output (such as Claude
Haiku 5.5, about $0.000075 per note-sized check) and a dedicated classifier are
both candidates; the setting lets a workspace switch without code changes.

## 2. Outcome signals on every answer

After each reply, the next message from the person in that thread (within six
hours), their reaction, and whether an approval was refused are classified into
one **outcome**: accepted, corrected, followed up, ignored, or not known. Each
request keeps its outcome and the notes and skills it used.

This is what makes everything else measurable: a note used in corrected answers
weakens and goes on hold; a note used in accepted answers grows; a skill's runs
can be compared before and after.

## 3. Memory: what gets better

| Missing today | Built in this plan |
| --- | --- |
| Search by words only | **Search by meaning** plus words, ranked by relevance and strength |
| Copies merge only with the same words | **Duplicates by meaning** merge in the nightly tidy-up |
| Contradictions found only when an agent says so | **Contradictions found** for every new note; the clear ones end the older note, close ones go to Needs your answer |
| Strength grows when a note is found | Strength grows when the **answer that used it was accepted**, falls and holds the note when it was corrected |
| Agent picks the kind; word rules for privacy | **Write-time checks** by the classifier |
| Given notes chosen by strength | Given notes chosen by **relevance to the request**, then strength |
| Failed searches listed one by one | **Grouped into themes** ("refund approvals: 12 searches") |
| Room notes seen only by owners and admins | **Members of a room see its notes** under Conversation context (read only) |
| Assistant memory on its own table | Moved onto the engine, so it fades and merges too |

## 4. Agents create their own skills

### Every three days, each agent checks its work

A controller task runs a **skill review** for each managed agent that did at
least 10 requests since its last review, at most once every 72 hours. It looks at
the last 30 days, so there is enough work to see patterns:

1. **Sort requests into job types.** Each finished request is classified into one
   of the agent's job types ("Send a payment reminder", "Quote for a repeat
   order"), or a new one, using embeddings and the classifier.
2. **Find routine work with no skill.** A job type is routine when it happened at
   least 5 times on at least 3 different days. The classifier checks it against the
   agent's skills: covered, partly covered, not covered.
3. **Decide.** For each routine job type:
   - **no skill and the runs were mostly accepted** → propose a **new skill** written
     from how the good runs were done;
   - **a skill exists but its runs were often corrected** → propose a **new version**
     with what the corrections taught;
   - **a skill's job stopped happening for 60 days** → propose **retiring** it;
   - otherwise nothing. At most two proposals per review.
4. **Draft.** The agent's own model writes the SKILL.md: when to use it, the steps,
   what to check, when to ask a person. It is given job steps (tool names and
   operations) from all its runs, but message text only from conversations shared
   with two or more people; private chats contribute only their job type and steps.
5. **Check.** The upload checks; no secrets; no names, addresses, long numbers or
   amounts (the lesson rule); only apps the agent already has; no scripts in an
   agent's own skill (scripts run on computers and need a person to add them).
6. **Rehearse.** The draft is tried on three earlier requests of that job type with
   the existing draft-only rehearsal runner (no tools, no actions). The classifier
   compares each rehearsal answer with what really happened: better, same, worse.
   A draft that does worse on any is dropped.
7. **Propose.** It appears on the agent's page under **Lessons and skills** for its
   owners and admins, with the evidence, the rehearsal results and the draft.

Every review is recorded, even when it proposes nothing ("Checked 41 requests in 6
job types: no new skill needed"), so people see the agent thinking about its work.

### A person approves; the skill says who made it

- **Approve** adds the skill to that agent, as one of its owners' own skills,
  marked **Created by Nova** with the date and the requests it came from. **Edit**
  changes it first. **Reject** keeps nothing and the same job type is not proposed
  again for 30 days.
- Every later version an agent writes says **Improved by Nova** with what changed
  and why; people's own edits say their name. History shows each version's author.
- Admins see all agent-created skills in the skills list with a filter **Created by
  agents**, and can share a good one with the workspace.
- A skill never grants a permission: apps, approvals and company rules decide what
  an agent may do, as today.

### After two weeks, the skill is measured

Its job type's corrections, follow-ups and steps before and after are compared and
shown on the skill ("Corrections on payment reminders: 4 in 20 before, 1 in 22
since"). A skill that made things worse is proposed for retirement.

## 5. Seeing an agent evolve

- **Agent page › Improving** (a new panel next to Recent work): the reviews every
  three days, skills it created and improved, notes it learned, and its learning
  score week by week.
- **Settings › Memory › Learning** (admins): the workspace's week: answers accepted
  and corrected, questions agents could not answer, notes learned, skills created
  and approved, and the agents that improved most.
- **Activity › a request**: "Used skill: payment-reminder (version 2, created by
  Nova)".

### The learning score

Per agent and per workspace, weekly, from outcome signals:

| Measure | Better is |
| --- | --- |
| Answers corrected, per 100 | lower |
| Answers accepted, per 100 | higher |
| Searches that found nothing, per 100 requests | lower |
| The same question asked again within 7 days | lower |
| Steps per request for routine job types | lower |
| Notes and skills approved; their measured effect | higher |

Shown as numbers with last week's beside them, never as one invented grade.

## Decisions

Settled by the owner: agents create skills from their routine work; people can see
which skills an agent created; agents check every three days.

**One decision for the owner: may an agent use a new skill before a person
approves it?** Recommended: **not by default.** Each agent has a setting
**When it creates a skill**: *Ask first* (default) or *Use after it passes
rehearsal* (the owner is told and can undo with one press). Admins can forbid the
second for the workspace under Agent rules. A skill changes how an agent works in
every conversation, so the safe default is review.

Proposed defaults, open to change: three days between reviews, 30 days looked at,
5 runs on 3 days to count as routine, two proposals per review, three rehearsals.

## Risks this accepts

- A classifier is wrong sometimes. Its confidence decides when a person is asked,
  every decision is stored, and the labelled set re-checks a model before use.
- Skills drafted from few runs can be too narrow. Five runs, three days and the
  rehearsal limit it; measurement after two weeks catches the rest.
- More model calls cost money. Classifier calls are small; skill reviews run at most
  every three days per agent; both count against the agent's model budget and stop
  when it is used up.
- Learning from shared conversations only (for text) learns less from private
  work; private chats still shape job types and steps.
