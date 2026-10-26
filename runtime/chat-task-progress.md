# Saved task progress in Chat

Implemented locally as a serial increment after [channel handoffs](channel-agent-handoffs.md),
PR 75 at `c997744d`. This contract and the selected OpenPencil concepts were saved before application changes.

## Product behavior

A person opens **Check progress** on an actual saved agent reply in its existing
channel, direct conversation or thread. A bounded drawer reads the controller's
saved task records: current phase, latest saved update, original requester,
accepted/started/finished times, pending input/approval/helper references and the
last successful check time. It does not send an ordinary message, start a new
model request, poll a model, restart work or estimate a percentage/finish time.
Completed and stopped work remains inspectable. Existing question, approval,
result and Stop routes retain their independent authorization and receipts.

The original human principal does not change. Reading status requires current
workspace membership, conversation visibility, private-agent visibility and every
current Library/event/handoff source guard. Reading an allowed channel task does
not require permission to request that agent or authorize an operation. An Owner
cannot read another person's private conversation merely by configuring its agent.
A direct task ID, altered workspace/chat, expired membership or revoked source
must fail closed. The new GET performs no lifecycle or audit mutations.

## Implementation and acceptance

1. Inspect the saved task projection and read boundaries; select a live OpenPencil
   document/page and save desktop/phone concepts before UI edits.
2. Reuse the existing Agent Work projection through a source-bound Chat GET,
   without changing agent-management permission requirements. Add only bounded
   question references, honest saved timestamps and a worker-released indicator.
   No schema migration or new executor infrastructure is required.
3. Show a shared-style drawer only after an explicit click on an actual task.
   Refresh while open through current chat events with a five-second visible-page
   fallback and a manual check. Keep one in-flight read, cancel stale responses,
   clean up listeners/timers and fence user/workspace/chat/reply changes. Matching
   source revocation immediately clears saved content; unrelated sources and the
   Chat composer draft remain intact. Network failure clearly labels a stale view.
4. Verify read visibility without a request grant, private/cross-workspace and
   source denial, wait/recovery/terminal states, no record changes on repeated GET,
   failed/stale reads, live invalidation and cleanup. Use real pinned Goose with
   scripted local inference to prove a connected progress/question/Stop journey,
   unchanged model-request counts while checking and mobile/dark interaction.
5. Run focused and risk-appropriate regression checks, inspect actual screenshots,
   update local status and publish one draft PR based directly on PR 75.

Free-form natural-language progress interception, notifications, time/cost
estimates, paid inference and cloud acceptance remain separate work. This slice
makes saved progress accessible; it does not claim the entire launch is complete.

## Executed local acceptance

`GET /api/identity/workspaces/{workspace}/chats/{chat}/messages/{reply}/progress`
reuses the Agent Work projection through a conversation-bound read boundary. The
route is PostgreSQL-only; the existing schema remains **29**. No deployment or
normal workspace database migration was required. History marks only actual saved
tasks with `has_task`; human messages, ordinary agent-authored handoff requests
and unavailable source output do not offer the control.

Verified: 101 root Rust unit checks, 219 disposable PostgreSQL checks (three new
progress cases), 161 web checks (four new behavior cases), Clippy with warnings
rejected, Rust/web builds, browser-test types and source-file limits. The new
PostgreSQL cases exercise read access without a request grant, wrong chat/tenant,
private agent, membership/Library revocation, input/approval/resume/recovery,
terminal timestamps and repeated reads that leave the saved task row unchanged.

Two connected process/browser journeys use real pinned Goose and scripted local
inference. A saved quote task moves from queued to input wait, releases its worker,
survives controller/executor restart and continues from an answer in the existing
question dialog. A channel observer without request permission reads the same
state and receives no Stop authority. The original person stops a second exact
task through the drawer. Repeated status checks add zero model requests; the whole
journey records five scripted model requests. Desktop, phone, dark and keyboard
focus/overflow checks pass with no page errors.

The second journey returns a source-protected helper result to its parent. Both
saved parent response and progress views clear on helper-read revocation; direct
parent/child progress reads fail after revocation. Regrant/restart starts no work
and does not restore the old source. This journey records four scripted model
requests and zero MCP calls. Opening/checking progress adds no inference. See
[design and actual captures](../designs/chat-task-progress/README.md).

The first browser run caught a heading accessible-name problem caused by a nested
status role. The final heading retains its native role with polite live updates;
the corrected journeys pass. Retained evidence is indexed in
`.tmp/chat-task-progress/verification.json`. These are local scoped checks, not
paid-provider or cloud acceptance, a delivery SLA or whole-product launch proof.
