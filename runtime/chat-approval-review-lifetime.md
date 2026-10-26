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

Two focused React behavior cases cover row removal/reappearance, zero writes on
viewing, source denial, late-read fencing and teardown. A connected local browser
check of channel approval and durable continuation is required before publication.
No schema, model, tool permission, cloud or normal database change is introduced.
