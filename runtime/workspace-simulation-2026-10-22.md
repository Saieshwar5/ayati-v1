# Workspace scenario audit — 2026-10-22

The Owner/User model passed the exercised scenarios after fixing an uncertain
invitation retry defect. This is local simulation evidence, not production certification.
Work is on `feat/unified-settings-owner-user`, following the approved Settings B design.

## Scope and isolation

The rehearsal used a new PostgreSQL database, `relay_simulation_workspace_audit_20261022`,
an empty simulation data directory and a debug server at `http://127.0.0.1:8878`.
The ordinary identity server, existing workspace data and prior design-review server
were left running and were not migrated or restarted. The ordinary server's simulation
endpoint was checked read-only and returned 404.

Twenty fictional people used one shared workspace with five teams, twenty scripted
agent identities, private channels, public announcements, cross-team channels and DMs.
Additional solo workspaces exercised workspace creation and switching. Real database
tests separately exercised the same person owning one workspace and being a User in
another, competing Owners, and membership/resource revocation.

Agents used deterministic fixtures. Email success, missing provider IDs, HTTP failures,
timeouts and failed provider cancellation used a local HTTP provider fixture. The live
simulation deliberately disables email delivery. No real invitation email, model call
or external business operation was performed.

## Results

| Area | Observed result |
| --- | --- |
| Automated tests | 69 frontend + 63 Rust library + 30 real PostgreSQL tests passed |
| Business workflow | 12/12 scenario groups; 773 sampled HTTP requests |
| Workspace API scenarios | 10/10 groups, including solo creation, role denial, independent roles, Owner races, CSRF, invitation uncertainty and revocation |
| Product browser journey | 6/6 groups: two-session messages, upload, recording, image/audio playback, feedback and narrow light/dark layouts |
| Settings browser journey | 8/8 groups: Owner/User visibility, last Owner, failed/stale saves, unconfirmed invitation, promotion/demotion, workspace switch, theme/keyboard/layout |
| Messaging failure/recovery browser journey | 8/8 groups: live delivery, delivered/seen, lost acknowledgement, offline send, unread threads, reconnect and revocation |
| Sustained activity | 1,040 accepted message sends over a 45-second offering window; zero send errors or missing checked IDs; 80/80 agent replies complete |
| Send latency in that load | Median 27 ms, p95 67 ms, p99 335 ms; local debug binary only |
| Twenty simultaneous agent requests | All accepted and completed; about 1.26 seconds to observed drain |
| Queue overflow | With dispatch paused, 64/80 requests accepted and 16 returned 429; ordinary chat remained usable; exact retries produced no extra message/job |
| Restart retention | 1,822 cumulative messages and 11 stored objects; selected channel message IDs, object checksums, workspace settings and people matched |
| Queued restart recovery | 20 accepted queued jobs survived an actual server restart and each produced exactly one completed reply |
| Coordinated backup/restore | Restored into a separate database; all 29 public tables and sequences matched; all 11 object files matched the coordinated file backup |

The 44 live scenario groups are separate from the 162 automated tests. Restart and
backup checks are additional exercises. API polling timing is not browser visibility
timing. This is a short workload on one local machine; it does not establish hosted
capacity, long-run memory stability or production rate limits.

Frontend build/check, browser-test type checking, Rust formatting, diff checks and
the 396-source-file length check passed. Clippy completed with the existing
`collapsible_if` warning at `src/identity/chat/runner.rs:81`; strict warning-free clippy
is not claimed. Browser tests actually executed; type checking alone is not counted.

## What the checks establish

- A solo workspace has one Owner and needs no extra roles or team setup. Concurrent
  exact creation retries return one workspace. Invalid names/IDs, another actor's
  request ID and changed creation payloads fail.
- The same account can have different roles in different workspaces. Workspace
  switching uses the selected membership and clears the old people view. Being an
  Owner elsewhere does not authorize reading or changing this workspace.
- Users keep personal Settings and Chat's people directory. People administration,
  invitations, settings changes and activity remain server-authorized Owner operations.
  Hiding buttons is not the authorization boundary.
- Owners have equal administration authority. Simultaneous self-demotion/removal
  retains at least one Owner. Simultaneous name edits reject a stale save; failed
  browser saves keep the draft and require explicit retry.
- Invitations require the matching account, confirmed provider delivery, current
  inviter authority and an unexpired pending invitation. Cancelled/expired/accepted
  invitations cannot be replayed to regain removed access. PostgreSQL additionally
  exercised twelve accept/cancel races with consistent membership outcomes.
- Membership removal immediately denies new protected reads/writes, cancels queued
  work and keeps shared history/attribution. It does not remove access to another
  workspace. Fresh re-invitation restores membership without restoring old App grants
  or issued App sessions. Role changes also invalidate those sessions.
- Private channel and file boundaries, wrong-channel attachments, exact message
  retries, altered retries, work approval revisions and revoked operation grants
  were exercised. Agent grants and business-operation authority remain separate.
- Outbox recovery handles offline and lost-response cases without duplicate storage.
  Seen counts and unread threads reflect the exercised visible conversation/thread;
  they do not prove comprehension or that an attachment was downloaded.

## Defect fixed and rehearsal corrections

An invitation whose send failed or timed out became `delivery_unknown`. The old
duplicate guard only blocked `pending`, so a second request could send another email
without resolving the uncertain first send. A failing HTTP-fixture test reproduced
this before the fix. SQLite and PostgreSQL now block both states until an Owner
cancels the outstanding invitation. No schema migration is required for this fix.

The UI now says **Delivery unconfirmed**, preserves the failed email draft and
refreshes the invitation list immediately, so closing the dialog exposes the
invitation for cancellation. Frontend and live browser regressions exercise this.

The older rehearsal also assumed that Users could approve work/update feedback and
that every top-level agent reply automatically belonged to the request's thread.
Those assumptions were stale. The corrected scripts assert User denial, have the
Owner approve, and create explicit request threads. The initial timeout was a
rehearsal correlation error: stored agent replies had already completed.

The removal scenario initially expected HTTP 400 for a gateway denial that correctly
returned 403. Its assertion was corrected and the remaining removal assertions were
resumed against the already-revoked account. Earlier passing groups were retained;
access was not restored to make the test pass. `workspace-report.json` records that
resumed verification. The original logs are retained as well.

## Recommended next work

1. **Complete the invitation experience.** Keep one invitation list with plain
   Pending, Delivery unconfirmed and Expired states, clear cancellation and deliberate
   re-invitation. Add a real email-provider staging journey before inviting customers.
   Cancellation prevents workspace access even if an old email still permits signup.
2. **Reduce Settings duplication.** People and Access currently repeat the same people
   list. Keep People with Invitations and manage each person's resource access in
   their details. Keep only the existing Personal/Workspace top-level tabs.
3. **Make actual access legible.** Show a concise per-person summary of shared agents,
   Apps and Home panels, and explain what promotion/removal changes. Preserve two
   membership roles; avoid adding manager/admin roles to solve resource-sharing UX.
4. **Complete workspace-owned connections.** Provisioning is currently unavailable.
   Keep Owner control and explicit resource grants when implementing it. Promotion
   still does not expose the original person's private adopted legacy runtime;
   private-runtime migration needs its own deliberate flow.
5. **Update role presentation sooner.** Server enforcement is immediate on each
   request, but the Settings role snapshot refreshes every 15 seconds. Use the
   existing live membership signal to refresh role presentation sooner, with tests
   for active dialogs and unsaved drafts.
6. **Validate deployment conditions.** Run a longer soak across several workspaces,
   database/provider outages, disk/connection pressure and real session/email flows.
   Add quotas and object reconciliation as deployment needs become clear. This run
   provides no evidence that a new broker or distributed worker system is needed.

## Retained evidence and rerunning

Local evidence is retained under `.tmp/workspace-audit/`, including test logs,
`backup/restore-result.json`, the coordinated backup and simulation data. Run artifacts:

`.tmp/workspace-audit/runs/2026-10-22T08-19-00.132Z-5e9611a2/`

This includes manifests, check reports, timing samples, browser screenshots,
`messaging-*/results.json`, retention and recovery reports. The binary hash and base
source commit are recorded in the run manifest; worktree fixes are preserved in this
branch's subsequent commits. Generated evidence and backup data remain ignored.

See [the simulation runner instructions](company-simulation.md#owneruser-workspace-follow-up).
Run workspace-removal checks last: fictional employee 19 intentionally ends without
membership in the original shared workspace while retaining their solo workspace.
Use a new isolated database for another complete 20-person rehearsal; do not reset
ordinary workspaces or re-enable removed access to reuse this audit.
