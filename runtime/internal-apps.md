# Local agent-built Apps

Apps can send a durable build request to an agent. Workspace tools save bounded
HTML, CSS, JavaScript and primitive collection schemas as immutable releases.
Only the creating agent can read or revise source. Creating an app gives it no
record access: each agent needs explicit per-collection read/write grants.
Record changes by agents additionally require exact single-use approvals.

The owner opens a preview, inspects the exact source and activates that release.
Preview records are separate from live records. An earlier active release stays
usable while another draft is reviewed. Live record operations run without a
model. Closing or navigating the app discards its browser record channel.

Generated documents have opaque sandbox origins in both iframe attributes and
HTTP CSP, including direct document navigation. Scripts and JavaScript form
events are permitted; network requests, native form submission, nested frames,
plugins and parent-page access are blocked. A trusted parent brokers only the
selected app/session's bounded record operations over a dedicated MessagePort.
The record session capability stays outside generated source. Provider keys are
never included. A document nonce alone cannot call record APIs.

Sessions expire after one hour. Activation invalidates prior live sessions;
archive invalidates sessions and agent source authority. CRUD uses optimistic
record revisions and durable UUID mutation receipts. Exact retries return the
saved outcome; conflicting reuse fails. Deletion leaves a tombstone/history.
Schemas add fields without dropping stored data; existing field types cannot
change. Code rollback preserves fields introduced by later releases.

Limits include 12 KiB source, 12 collections, 24 fields per collection, 4 KiB per
record, 1,000 record IDs per collection and 10,000 mutations per app. Fields are
optional primitive strings/numbers/booleans. There is no custom backend code,
SQL, external networking or arbitrary package installation in generated apps.
History and retained code/data are separate; backup/restore is not yet delivered.

This is a local SQLite-backed lifecycle proof. Supabase and Vercel remain the
chosen cloud direction; neither integration, hosted access nor deployment is
implemented. Local activation is not publication. Member authentication remains
deferred by the owner; the existing daemon's local-owner boundary still applies.

`tests/internal_apps.rs` covers release review, schema compatibility, durable
records, grants, expiration and preview isolation. `tests/internal_apps.mjs`
uses real Goose and a browser to create a functioning inventory form, activate
it, persist records without model calls, check sandbox boundaries, revoke agent
access and restart with saved live records. It uses synthetic data and models.
