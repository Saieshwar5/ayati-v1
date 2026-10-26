# Exact approval review during conversation refresh

A Chat conversation owns its open approval drawer independently of a message row.
A membership/access refresh may temporarily clear and reload the transcript without
closing the person's review. The drawer still clears exact fields immediately on
access invalidation and reads the current authorized action again. A denied read
removes decision controls; stale responses cannot restore private fields.

Changing conversation, account/workspace or leaving Chat tears down the viewer.
The existing original source, private-agent, designated approver, current rule and
exact-action checks remain authoritative. Keeping a dialog open grants no ability
to approve. Other screens retain their existing locally owned review dialogs.

Verified independently on the clean prerequisite branch: 163 web checks (two new
lifetime cases), Rust/web builds, browser-test types and all 809 source-file limits.
Two connected browser journeys using real pinned Goose with scripted inference
pass channel review across membership refresh, authorized User decisions,
restricted observer reads, source revocation, saved receipts, executor replacement,
controller restart, request changes, Stop and uncertain-write retention. The old
resuming-label assertion now follows the current Saved update wording.

The review remains open through the harmless reference refresh; it does not retain
private fields after an access denial. Records are indexed in
`.tmp/approval-review-refresh/verification.json` before publication.
No schema, model, tool permission, cloud or normal database change is introduced.
