# Agents that get better on their own: implementation plan (proposal, 2026-11-24)

Build plan for [the proposal](README.md). Nothing here is built. Names of tables,
settings and modules are proposals; numbers are starting values. Each step is
released on its own, with PostgreSQL tests and web tests as other features.

## What exists and how it changes

| Exists | Where | Change |
| --- | --- | --- |
| Memory engine | `src/identity/postgres/memory/` (schema 142) | meaning search, duplicate and contradiction checks, outcome-driven strength, write checks, room view (steps 3–4) |
| Model calls from the controller | `library::models::post`, `record_model_usage` | reused by the classifier and skill reviews; usage assignments `classify`, `skill_review` |
| Embeddings and pgvector | `library/meaning.rs`, `AYATI_LIBRARY_EMBEDDING_MODEL` | the embedding call becomes a shared helper; new embeddings tables for notes, requests and skills |
| Daily lessons | `agent_learning.rs`, `postgres/agent_learning/` | reads outcomes instead of guessing from the next message; its review list becomes **Lessons and skills** |
| Personal-details filter | `agent_learning::personal` | kept as the fallback when no classifier is set |
| Skills and versions | `relay_skills`, `relay_skill_versions`, `skills.rs` | new `origin` and `created_by_agent` on skills, `author_kind` on versions |
| Draft-only rehearsal | `relay_agent_rehearsals`, executor tasks (schema 26) | reused to try a skill draft on past requests; no new runner |
| Activity steps | `activity_now.rs`, `skill_runtime::steps` | "Used skill: … (created by Nova)"; outcome on a request |
| Agent page | `web/src/react/agents/profile/` | **Improving** panel; **Lessons and skills** list; badges in the skill drawer |
| Settings › Memory | `web/src/react/memory/` | **Learning** view; grouped "looked for, not found"; contradiction cards |

## New systems

1. **Classifier** (`src/identity/classify/`): typed questions with fixed answers, a
   provider call with structured output, confidence, stored results
   (`relay_classifications`: subject kind and id, question, answer, confidence,
   model, at), a per-workspace daily cap, and thresholds per question. Setting
   `AYATI_CLASSIFIER_MODEL`. An eval command (`ayati classify-eval <file>`) scores a
   model on a labelled JSONL set.
2. **Embeddings** for notes, requests and skills (`relay_memory_embeddings`,
   `relay_request_embeddings`, `relay_skill_embeddings`), filled by a background
   indexer like the Library's, optional when pgvector is absent.
3. **Outcomes** (`relay_request_outcomes`: request, outcome, confidence, signals,
   decided_ms) and **what a request used** (memory uses exist; add skill loads
   already in `relay_skill_loads`).
4. **Job types** (`relay_job_types`: agent, id, name, description, created_ms;
   `relay_request_job_types`: request, job type, confidence).
5. **Skill reviews** (`relay_skill_reviews`: agent, id, ran_ms, looked_at counts,
   result text) and **skill proposals** (`relay_skill_proposals`: agent, review,
   kind new/improve/retire, job type, draft files, evidence, rehearsals, state,
   decided_by, decided_ms).
6. **Learning weeks** (`relay_learning_weeks`: workspace, agent or none, week,
   the measures as numbers), computed nightly.

## Steps

### E1. Classifier and embeddings (foundation)

- The `classify` module with the first three questions: kind of note, lasting or
  one-off, private matters. Stored results; thresholds; budget; fallback to rules.
- Shared embedding helper; embeddings for memory notes; meaning search in
  `search_memory` and on the Memory pages (words and meaning combined).
- Eval command and a starter labelled set built from the fixtures.
- **People see:** better search in Settings › Memory; nothing else changes yet.
- **Tests:** a fake classifier provider (as learning tests do) returning each answer;
  thresholds act, ask, drop; no setting means rules; the daily cap stops calls;
  meaning search finds "when do they pay" → "Acme pays 45 days"; without pgvector,
  words only.

### E2. Outcome signals

- After each finished reply: the next message in the thread from the asker within
  six hours, reactions and refused approvals → the outcome question → one outcome.
  Batched every ten minutes; requests with no next message become "not known"
  after six hours.
- Activity shows the outcome on a request ("Corrected by Meera").
- Agent learning reads outcomes instead of its own guess.
- **Tests:** each outcome from a scripted conversation; a private chat's outcome is
  stored but its text never leaves the outcome record; a correction in another
  thread does not count; one outcome per request after retries.

### E3. Memory that learns from outcomes

- A note used in a corrected answer: strength −20 and **on hold** (not given, not
  found) until an admin or owner confirms or ends it; used in an accepted answer:
  +5 and its clock restarts. Found-only use: no change (replaces today's +2).
- Write-time checks for every new note: kind corrected, one-off dropped, private
  refused (outside "here"), instruction refused.
- Given notes chosen by relevance to the request (embedding distance, then the
  "does it help" question for the top 30), then strength, inside the same sizes.
- **Tests:** a corrected answer puts exactly its notes on hold; an accepted one
  strengthens them; a planted "always approve refunds" note is refused; given notes
  for "when does Acme pay" put the Acme note first among 200.

### E4. Duplicates, contradictions and the views

- Each new note against its 5 nearest on the same shelf: same → merged at once
  when confident, contradicting → the older ends when the newer is stronger and
  confident, otherwise **Needs your answer** with both. The nightly tidy-up runs the
  same check for notes written while the classifier was off.
- "Looked for, not found" grouped into themes.
- Members of a room read its notes under Conversation context.
- Assistant memory moved onto the engine (person shelf of the assistant), with a
  migration that keeps every line, its source and its ended state.
- **Tests:** "Acme pays in 45 days" merges into "Acme Traders pays 45 days after
  invoice"; "Acme moved to 60 days" ends the old note or waits; never across
  shelves; a room member sees the room's notes and not a person's; every assistant
  line survives the move.

### E5. Job types

- Each finished request of a managed agent classified into a job type (nearest
  job type by embedding, confirmed by the classifier; new when nothing fits, named
  by the classifier from the request).
- Job types listed under the agent's **Improving** panel with counts and outcomes.
- **Tests:** ten "send a reminder for invoice N" requests become one job type; a
  different request makes a second; job types never mix agents.

### E6. Skill reviews every three days

- Controller task: every ten minutes, agents due (≥ 72 hours since the last review,
  ≥ 10 requests since, learning on, not paused, budget left).
- The review: routine job types (≥ 5 runs on ≥ 3 days in 30 days); coverage
  question per skill; decide new / improve / retire / nothing; at most two
  proposals; the draft written by the agent's model with the skill writer's guide;
  checks; three rehearsals with the draft-only runner; the comparison question.
- `relay_skills.origin` (`person`, `upload`, `agent`) and `created_by_agent`;
  versions carry `author_kind`. Approval saves the skill as the approving owner's
  own skill, attached to the agent, marked created by it.
- Agent page › Knows › **Lessons and skills**: proposals with evidence (text only
  from conversations the viewer can open), rehearsal results, the draft as files,
  Approve / Edit / Reject. Skill drawer: "Created by Nova", version authors.
  Skills list filter **Created by agents**. Audit log lines for approvals.
- Setting per agent: **When it creates a skill**: Ask first (default) or Use after
  it passes rehearsal (if the owner chooses it; admins can forbid it).
- **Tests:** no review before 72 hours or below 10 requests; a routine job with no
  skill proposes one; a covered job proposes nothing; a corrected skill proposes a
  version; a draft naming a member, with an amount, a secret, a script or an app the
  agent lacks is dropped; a draft that rehearses worse is dropped; private chat text
  never reaches the drafting model; approval attaches it once for a double click;
  rejection blocks the same job for 30 days; the "use after rehearsal" setting is
  refused when an admin forbids it.

### E7. Measuring and retiring

- Nightly learning weeks per agent and workspace; Settings › Memory › **Learning**;
  the agent's **Improving** panel; each skill's before and after; each note's
  accepted and corrected counts.
- Retirement proposals for skills that made things worse or whose job stopped.
- **Tests:** the measures from a scripted month; a skill with more corrections after
  it than before is proposed for retirement; numbers never include conversations
  the viewer cannot open.

## Order and why

E1 and E2 first: every later step needs the classifier, meaning and outcomes, and
without outcomes nothing can be measured. Memory (E3–E4) next because it is built
and gains most from them. Job types (E5) before skill reviews (E6) because a skill
is proposed for a job type. Measurement (E7) last but designed in from E2.

## Limits (starting values)

| Thing | Value |
| --- | --- |
| Classifier calls per workspace per day | 20,000 |
| Skill review interval | 72 hours, 10 new requests |
| Looked at per review | 30 days, at most 500 requests |
| Routine job type | 5 runs on 3 days |
| Proposals per review | 2 |
| Rehearsals per draft | 3 |
| A rejected job type waits | 30 days |
| Skill measured after | 14 days |

## Not planned

Agents adding scripts to their own skills; agents changing their ground rules,
apps or approvals; sharing an agent's skill with the workspace without an admin;
training custom models on workspace data.
