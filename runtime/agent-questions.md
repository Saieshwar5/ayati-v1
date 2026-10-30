# Durable agent questions

This increment adds ordinary missing-facts questions to the
[saved task lifecycle](durable-approval-waits.md), after
[Agents workspace PR 67](https://github.com/Saieshwar5/ayati-v1/pull/67).
It requires PostgreSQL **schema 23**. Questions remain separate from
[exact business approvals](channel-approvals.md).

An agent may ask the original requester for missing business facts using single
choice, multiple choice or a short flat form. The controller binds the question to
the original workspace, agent, conversation/thread, task and respondent. Model
output cannot choose another identity or business approver. Further respondent
delegation, group voting, nested forms and arbitrary generated UI are deferred.

One versioned schema uses stable question/field/option keys. Initial engineering
bounds are ten questions per task, one pending question at a time, eight form
fields, twelve options, 16 KiB specifications and 8 KiB answers. Text, number,
date, boolean and choice fields have explicit server validation. A date is a
calendar date, not a timezone-free timestamp. Explicitly enabled free-text
alternatives remain separate from closed option IDs. No executable content or
submission URL is accepted. Connection credentials belong in protected account
setup rather than these forms.

Creation and answers persist in PostgreSQL. An exact retry returns its saved
record; changed content under the same question key or submission identity is
rejected. Initial specifications are immutable at revision 1. A deliberate
follow-up is a new question; changing an old answer cannot rewrite dispatched
work. Questions and answers inherit current conversation and Library access.
Only the intended requester can answer or decline; an admin cannot answer for
another member merely because they manage the agent.

The existing task/checkpoint lifecycle saves the question before worker release
and resumes the original task once all input and action waits settle. An early
answer is retained even before release. Waiting holds no model loop or execution
slot. Current source access, claim generation, Stop and uncertain external effects
remain independent gates. Decline records no answer; it is distinct from a No
option or stopping the task. Stop/access invalidation disables pending questions
without reviving them on regrant. Ordinary answers never approve business effects.

The UI uses compact question references in the original reply and one bounded
review dialog with familiar controls. Opening is read-only. Failed validation or
saves retain the local draft; duplicate submissions are guarded synchronously.
Saved/declined/invalidated questions stay inspectable with current access. Stale
reads and unmounted dialogs cannot reveal another identity's data.

The execution adapter targets PostgreSQL managed private/workspace agents on
pooled executors through authenticated worker control. Exact approval cards continue using their existing gateway and routing.

## Tool and answer contract

`ayati_control__ask_question` accepts `{ key, specification }`. The key is 1–64
ASCII letters, digits, underscores or hyphens. The controller creates the saved
identity and binds all authority from the current authenticated task/claim.
Specifications use version 1 and `kind: single_choice|multiple_choice|form`.
Title, description, fallback, stable options/fields and explicit bounds are data.
Unsupported keys/types/versions return validation feedback and create no controls.
After asking, the agent explains the wait and ends its attempt. New business
operations are denied while a question remains pending; receipt reads still
reconcile already requested work.

Choice answers use `{ choices: [optionId], other?: text }`; a single choice has
exactly one selected option or enabled alternative. Multiple choice uses its
declared selection bounds. Forms use `{ fieldId: value }`, with unknown keys
rejected. A required boolean needs an explicit Yes/No value; false and numeric
zero are valid answers. Suggested values are visible and editable, and Submit
is always explicit. Text limits count characters, alongside byte payload bounds.

`GET /api/identity/workspaces/{workspace}/questions/{id}` returns the currently
authorized specification, state and saved answer. `POST` to the same path accepts
`{ id, revision, decision: answer|decline, values }`, where `id` is the submission
UUID. Decline requires an empty values object. Exact retries recover the saved
receipt even after continuation; a changed payload cannot reuse that identity.
JSON object key order does not change meaning, including after JSONB storage.
Business-action receipt lookup likewise compares the exact semantic operation
and arguments, including records written by previous binaries, so reconstruction
cannot dispatch the same fields again just because object key order changed.

Chat history/SSE batch-project bounded references, not answer bodies or assembled
prompts. The latest question is compact; earlier references remain expandable.
Opening its details rechecks conversation and Library access. Only the designated
requester with current task authority can answer. An observer may inspect ordinary
source-visible facts without gaining response or business-operation authority.

Close/Escape dismisses a view without answering. Decline supplies no facts and
allows the task to report missing requirements. No is a real choice/boolean
answer. Stop permanently ends the task and invalidates pending questions. Escape
inside a question closes only that dialog and restores focus to its launcher,
including when the launcher is inside a thread dialog.

## Persistence, release and recovery

`relay_task_questions` records immutable specifications, task/cycle/key, state,
answer, respondent, submission receipt and timestamps. The original requester is
derived from the saved task rather than accepted from the model. A question
transition and its possible continuation share the workspace transaction. The
checkpoint waits for pending input as well as approvals; executing/uncertain
external actions still hold continuation. Early answers and release/answer races
cannot lose the response or enqueue two cycles.

Task termination invalidates pending questions through a database trigger.
Revocation includes a saved question even before its first checkpoint, preventing
revoke/regrant races from reviving it. Old worker claims cannot publish into a
resumed/stopped task. Library revisions protect metadata, details, history and
replayed references. Saved facts and checkpoint text stay untrusted during fresh
context reconstruction. The existing 20-cycle and 128 KiB continuation bounds
remain; oversized combined context fails explicitly instead of dropping facts.

## Adapter coverage and limits

| Path | Local evidence |
| --- | --- |
| PostgreSQL managed workspace agent, channel/thread | Real Goose form, multiple choice, saved answer, restart/replacement and separate exact write approval |
| PostgreSQL managed private agent, private DM | Same renderer and gateway; closed choices, No, decline, Stop, private access and no business connections |
| Scripted company simulation | Deterministic simulation replies; not evidence of Goose question handling |

No respondent reassignment, question editing, answer revisions, expiry, nested
forms, voting or automatic arbitrary text-to-answer conversion is introduced.
Selecting a consequential-looking label in a form never changes gateway policy.
Hosted/commercial-model quality and cloud capacity are separate acceptance gates.

## Upgrade and verification

Stop old binaries and back up identity data before explicitly applying
`m20261026_000023_agent_questions` with the existing administrator migration/grant
procedure. This slice introduced schema 23; current startup requires the full
[Chat organization schema 24](chat-organization.md) and never applies migrations automatically.
Upgrade tests preserve a schema-22 approval wait/checkpoint and its later wake-up.
The ordinary workspace database was not migrated during implementation.

Focused checks are `cargo test --locked --lib identity::interactions` and
`cargo test --locked --lib question_ -- --ignored --test-threads=1`, with a
disposable `RELAY_TEST_DATABASE_URL` for the latter. They cover schema/value bounds,
wrong respondents, cross-scope and Library denial, early answers, key-order retries,
answer/release/Stop races, source revocation before/after release and migration.
`web/tests/agent-questions.test.mjs` covers explicit mutations, failed drafts,
duplicate submissions, stale/denied reads, closed/enabled alternatives, fallback
and nested dialog dismissal.

`RELAY_TEST_DATABASE_URL=... node --test tests/agent_questions.mjs` runs real pinned
Goose with a scripted local model, PostgreSQL, Playwright and authenticated MCP
records. It verifies all supported field shapes, source/thread restoration,
worker release and another task, no idle model polling, restart and replacement,
ordinary answers followed by independent exact approval, one write/verification,
private managed agents, decline/Stop/revocation and keyboard/phone/dark UI.
The local-broker case uses an isolated compatibility worker; it is not a hosted
authentication acceptance test. Retained screenshots are in
[the design/evidence record](../designs/agent-questions/README.md).
No cloud resource or paid inference is used.
