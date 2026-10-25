# Saved approval waits

The [editable OpenPencil concept](waiting.pen) and [source JSX](waiting.jsx) were
created in a fresh document before changing the status UI. Existing documents
were preserved. The concept keeps the current channel, saved approval drawer
and Stop control, adding waiting, resuming and uncertain-outcome states.

Actual local browser journey captures:

- [Waiting on desktop](waiting-desktop.png)
- [Waiting on a phone](waiting-mobile.png)
- [Decision saved, continuation queued](queued-continuation.png)
- [Original task continued](continued-task.png)
- [Uncertain outcome in dark mode](uncertain-wait-dark.png)

The journey uses synthetic company data, a scripted model and an authenticated
test MCP. [The runtime contract](../../runtime/durable-approval-waits.md) records
what is verified and what remains open. Captures are inspected before delivery.
