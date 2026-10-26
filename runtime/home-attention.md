# Home and owner attention

This contract describes the retained native/private-runtime attention adapter.
PostgreSQL-managed workspace questions, exact approvals and outcomes now use
[personal Home attention](workspace-attention.md), with their own current-source
audiences and read/review receipts. The private inbox remains explicitly linked.

Start Ayati normally, then choose **Home** in the left rail or open
`http://127.0.0.1:8765/?view=home` (use your configured port). **Agents** returns
to chat and agent creation. The root URL opens Home; `/?agent=local-agent` opens chat.

Home shows every registered agent, including preparation failures, and combines:

- Active and queued requests, with Stop or Cancel for the exact request.
- Pending approvals with their original title. Expand Review action to see the
  complete payload and decision controls.
  Allow once and Deny use the existing single-use server decision boundary.
- Failed and interrupted requests until the owner chooses **Mark reviewed**.
  Reviewing only clears attention; it never changes the task outcome, deletes
  history, retries work, or claims that an external action was successful.

Each card names its agent. **View request** loads that request's saved messages,
including requests older than the recent chat window. **Open conversation** opens
that agent's conversation, where the owner can send a deliberate follow-up.
Request history is a point-in-time read; reopen it to refresh a running request.

## Refresh and recovery

Visible Home tabs poll every 2.5 seconds and refresh on focus/reconnection.
Refresh requests do not overlap. An unchanged snapshot preserves card DOM/focus.
Failed refreshes disable decision buttons; stale views refresh before accepting
another click. A decision is never automatically retried after an uncertain
transport result. The next snapshot reconciles its actual durable state.

A second tab may still show an old approval briefly. The server rejects it if
already decided, stopped, or attached to an obsolete run claim. Restart cancels
pending approvals and turns started work into interrupted outcomes; those outcomes
remain in Home until reviewed. Queued work follows the existing recovery contract.

## API and storage

All routes retain the loopback/same-origin guard and `Cache-Control: no-store`.
This is a single-owner local dashboard. Agent selection is not authentication.

- `GET /api/home`: all registered profiles and attention per ready agent.
  Each agent's attention is read in one SQLite transaction. Separate agent
  databases do not provide a simultaneous cross-agent transaction.
- `GET /api/tasks/{id}/history`: saved task and messages for the selected agent.
- `POST /api/tasks/{id}/review` with `{}`: acknowledge a failed/interrupted
  outcome. Idempotent for an already reviewed eligible task; rejects active,
  completed, stopped and foreign/unknown tasks.
- Existing `POST /api/permissions/{id}` and `POST /api/tasks/{id}/stop` handle
  decisions and cancellation. Home always sends the exact `X-Ayati-Agent` header.

Schema 6 adds `attention_reviews` in each agent database. Existing histories and
runtime mappings are preserved. Older binaries reject the newer schema; do not
restart a pre-schema-6 binary against migrated state.

Home queries unresolved work independently of the last 100 tasks/200 messages.
Each agent returns its oldest 20 unreviewed outcomes and the full remaining count;
reviewing outcomes reveals the next ones. Request previews are 240 characters;
View request provides the saved full messages. Active work retains the existing
32-request queue bound. No new dependency or inference call is needed by Home.

## Limits

Settings now shows appearance and local configuration status. Credential editing,
staff access, connected apps, automatic retries, all-history pagination and a
structured question/answer inbox remain future work.
Approvals still display the exact runtime JSON; friendly tool-specific summaries
can be added while preserving the complete payload and decision identity.
