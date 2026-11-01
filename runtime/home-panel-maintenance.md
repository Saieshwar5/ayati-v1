# Personal Home panel maintenance

Personal-panel increment, 27 October 2026. The contract and OpenPencil concept
were saved before application changes. Local implementation and verification are
recorded below; this does not claim live-provider or cloud acceptance.

## Product agreement

Home is each person's visual overview of their responsibilities. Its standard
clock/location and Notifications area remain; notification contents and custom
panels remain permission-aware and personal. Every panel has one human owner,
one responsible agent and its original source conversation.

The owner reviews a saved panel before enabling updates. The agreement names the
agent, purpose, existing source reads, task instructions, optional fixed interval
and maximum age. Enabling updates grants no new source, operation or external-write
authority. Manual Update now and scheduled updates use the ordinary durable queue
and shared scheduler. Opening Home starts no agent work.

This first increment supports only panels with completed, currently authorized
read receipts. The reviewed exact connection/operation/arguments and authority
versions form the update ceiling. Broader filters, rolling argument templates,
Library-only updates and event bindings require later reviewed contracts.

## Update boundary

Maintenance is a controller-bound task, never a model-selected panel owner.
Only reviewed reads and bounded panel publication are available. Reads require
current grants and the exact reviewed arguments; write operations and helper
handoffs are denied. A separate refresh tool supplies only a section for the
reviewed template: the controller fixes title, purpose, owner, agent, audience
and source conversation. Every required source read must complete in this run.

The [general panel families](general-home-panels.md) share this update path.
Values and records may refresh; reviewed labels, units, chart variants/series,
table columns/types, comparison direction and progress targets remain fixed until
a new definition review.

Routine snapshots publish automatically within that agreement. Definition changes
remain ordinary drafts requiring Keep on Home, and invalidate the old agreement.
Pause, permission changes and agreement replacement fence later publication.
Duplicate update requests coalesce; exact retries acknowledge their original run.
An older run cannot overwrite a newer result.

## Persistence and health

Keep immutable human-reviewed definitions separate from routine data snapshots.
Retain a bounded recent snapshot history with durable run/publication receipts;
routine updates do not consume the existing 100-definition-revision budget or
append every historical observation to the current snapshot's provenance.
Each update retains both its reviewed basis and its own source/context guards.

Display last successful source-read completion separately from snapshot save time,
next scheduled check and current run outcome. Read completion proves an observed
operation, not factual interpretation or the upstream system's business coverage.
Keep the last authorized complete snapshot on failure. Hide content when its
source access is revoked. No failed read becomes an empty business result.

Updates continue while Home is closed. Visible Home uses its existing five-second saved-read poll for routine data and
health updates, preserving open form drafts. Source/access changes retain immediate
personal invalidation; reconnect rechecks authoritative data.
Personal arrangement/hiding never changes maintenance authority.

## The panel frame (schema 57)

Since 2026-11-01 every published panel on Home carries the same frame
(people-and-agents-first, Phase 1e). A panel only shows; every decision opens Activity.

- **Kept by <agent> · for you · Updated 20 min ago · sources read 10:22.** Times come
  from the snapshot, not from the agent.
- **What changed.** `refresh_home_panel` takes an optional `what_changed`: one or two
  plain sentences, at most 280 characters, only when something meaningful changed (a
  number moved noticeably, a target was crossed, something needs a person, or a source
  could not be read). It is saved with that snapshot (`relay_home_snapshots.what_changed`,
  migration `m20261101_000057_home_frame`) and shown below the panel's numbers inside
  its scrolling content. A blank note is stored as none; drafts never show one.
- **Freshness from the server's status.** `delayed` (older than the agreement's maximum
  age) says "Not updated for 3 h. These numbers may be out of date." and dims the
  numbers; `failed` says the last update could not complete and the previous result is
  shown; `blocked` says access to a source changed. Delayed and failed link **How to
  fix it →** to the latest update (`maintenance.last_update`) in Activity.
- **N need you →** counts the waiting approvals and questions from this panel's agent
  that the viewer can act on, from Home's one `needs_you` read (shared with the
  Notifications panel, which now reads "2 need you →"). It opens Activity → Needs me.
- **Show proof** opens the update that produced these numbers (`snapshot.task`) in
  Activity; **Ask about this** opens the person's direct chat with the agent, starting
  "About the <title> panel: " and keeping any existing draft.

## Delivery and acceptance

1. Save this contract and desktop/phone design before application UI edits.
2. Add an explicit forward PostgreSQL migration, agreements, bounded snapshots,
   exact run receipts and existing-scheduler bindings. Migrate disposable fixtures
   only; do not migrate the normal workspace or activate cloud/provider services.
3. Add source-safe owner APIs, controller-bound refresh tools and read ceilings.
4. Add review/setup, Update now, Pause/resume and honest health to personal panels.
5. Prove wrong owner/tenant, stale writes, pause/revocation races, exact/semantic
   retry, required reads, write denial, bounded history and restart preservation.
6. Execute the real Goose/scripted-provider browser journey, responsive and theme
   checks, focused plus appropriate regressions, source limits and diff inspection.
7. Publish one focused PR with evidence and remaining work.

Event-triggered refresh, deterministic aggregation, shared panels, reassignment,
calendar recurrence, notification preferences, real-provider latency/usefulness,
per-panel spending ceilings, cloud capacity and production backup/restore
acceptance remain separate work. Scheduled updates can consume model inference.

## API and operational bounds

Schema **33**, `m20261027_000033_home_panel_maintenance`, adds agreements, private
schedule bindings, exact update-run receipts and independent routine snapshots.
Controllers/executors must use this build; installed older guest images do not
gain tools through a frontend update. Startup never migrates automatically.

Below `/api/identity/workspaces/{workspace}/home-panels/{panel}`:

- `POST /maintenance`: exact request UUID, expected agreement revision, approved
  panel revision, enabled state, instructions, optional interval and maximum age.
- `POST /refresh`: exact request UUID and expected agreement revision. Coalesces
  with current unfinished maintenance; uncertain writes are never blindly retried.
- Existing list/review/layout APIs retain their original ownership/source guards.
  Lists include the current authorized snapshot and personal agreement/health.

Only the panel owner can authorize its upkeep. They need independent permission
to request its managed agent; this grants no right to manage that agent. Private
maintenance schedules cannot be controlled through another agent maintainer's
general Automation APIs. Retired schedules remain paused and cannot regain panel
authority. Updating/pausing an agreement cancels its unstarted tasks and fences
publication by running old-policy tasks.

The interval is 60–86,400 elapsed seconds. Maximum age is 60–604,800 seconds and
at least the interval. Instructions are at most 2,000 bytes; source ceilings have
1–16 distinct exact allowed reads. Existing technical agent-routine and queue
bounds apply. Retain the newest 32 routine snapshots, with durable run/publication
receipts. These limits are operational guards, not subscription entitlements.

Controller-bound `home_panel_update_context` provides the reviewed definition and
exact reads. `refresh_home_panel` accepts one validated section; headings/columns,
template, title/purpose, identities and source scope remain fixed. Other business
tools, history mutation and handoffs are denied for maintenance. Routine output
retains the original definition's source guards and its own current provenance;
its private replies cannot enter another person's shared transcript, saved reader
or ordinary recent/history context. Regranting sources cannot relabel old receipts.

After a human approves a changed definition, Home offers a fresh agreement review
against that definition's reads. Update now remains disabled until renewed. This
keeps definition review and standing update authority separate.

## Verification

The initial connected journey passes with pinned real Goose and scripted local
inference: member-owned setup without agent management rights, one manual and one
scheduled refresh, second-device reading, controller restart, pause, source
revocation and private admin denial. It records 11 model requests, three source
reads and zero external writes. Captures and reproduction are indexed in [the design record](../designs/home-maintenance/README.md).

Storage checks include 102 refreshes without consuming definition revisions,
32-snapshot retention, per-run provenance, required reads, write/argument denial,
exact/coalesced retries, stale policy fences, changed-definition recovery, honest
source age, member authority and populated-v32 migration with a pending question.
These are controlled local fixtures; receipts do not prove model interpretation.


Final local checks: 268 disposable PostgreSQL regressions and seven focused cases
rerun after refinements; 104 root Rust, nine guest, 187 web and 75 offline Python
checks; warning-denying Clippy, Rust/client builds, browser-test types, 895 source
limits and diff checks. Four connected journeys cover maintenance, existing panels,
brief reading and local workspace restore. The final maintenance rerun also keeps
an unsaved agreement draft intact while the scheduled snapshot arrives.

Reproduce the new journey with `node --test tests/home_panel_maintenance.mjs`,
using the built binary/client, pinned Goose and `RELAY_TEST_DATABASE_URL` for a
loopback disposable `relay_simulation_*` database. Storage cases run with
`cargo test --locked --lib home_maintenance -- --ignored`. Retained local logs and
check index: `.tmp/home-maintenance/`; final connected evidence:
`.tmp/independent-worker-a5iWr3/`. No real provider or cloud service was activated.
