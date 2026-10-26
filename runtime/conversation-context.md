# Trusted requester and attributed conversation context

Managed PostgreSQL Chat prepares a fresh, bounded Goose context for each attempt.
[Managed runtime guidance and host isolation](managed-context-boundary.md) exclude
unrelated host project hints and local-owner capability instructions.
The controller identifies the human requesting principal separately from message
content. An authenticated task, not a name in a prompt, determines the principal.

## Current behavior

Newly queued messages store an attributed JSON transcript: stable message and
author IDs, human/agent kind, sequence, timestamp, thread, content and display name.
The explicit request message ID identifies which message the task answers. A
display name is a profile value observed when assembling context; it is neither a historical
identity guarantee nor an authorization claim. Distinct people may share a name.
Other speakers' email addresses are not included in the transcript.

Before each managed execution, current stored requester ID, display name and
email are assembled with workspace, agent, task and conversation identifiers.
The exact running claim, current membership, conversation access, agent grant and
source rights must still pass. Current identity can reflect a rename while the
durable intake snapshot retains its earlier profile value. Execution rebuilds
eligible recent evidence from saved messages up to the original task's reply
sequence; later messages never displace the original request. It reapplies the
current joining policy and source exclusions without rewriting the queued job.

The requesting principal is the human responsible for the task. `origin` separates
human messages, agent handoffs, background automation and historical requests.
`initiating_actor` identifies the actual human or agent sender when known; it is
null for background automation and historical unknown origin. A helper message is
stored with its sending agent as author from the start, while its human principal
remains unchanged. This does not claim a human personally typed an automation's
message or let a sending agent acquire that person's authority.

Profile values and bodies remain untrusted data. Identity
context grants no action, approval, source access or verified external-customer
identity. The model is instructed not to repeat private contact details without a
relevant authorized purpose; actual business operations still pass server checks.
This improves attribution, not a guarantee against every model disclosure error.

Managed recent history remains scoped to the current conversation, at most 60
messages. Revision-zero profiles retain their existing 30-message/thread selection.
The 14,000-byte budget now counts serialized attribution and escaped content. The
latest explicit request remains intact if it alone exceeds that budget, bounded
by the existing 12,000-byte input limit and prepared-context limit. Older messages
are removed first. The existing since-joining policy remains.

General recent history and older-history retrieval exclude Library-derived
answers, the initial source question/title in a Library-created conversation,
business-operation answers, event replies and helper requests/results. Those
records need their source-aware paths; ordinary conversation membership must not
silently renew their source authority. Conversation titles are omitted from the
model's identity envelope because a Library conversation title can itself reveal
the source's name. User-authored facts in ordinary messages are still conversation
content; this is not an automatic classifier for sensitive payroll/customer data.
The current task's exact original request is included after its own authority and
source checks, including a helper's authorized instruction. A Library task uses
its canonical question separately from explicit authorized sources, rather than
copying the display message's generated source link into the question.

A [read-only context preview](conversation-context-preview.md) makes the current
conversation selection visible in Agents → Knowledge. It does not impersonate
another requester or claim to reproduce a historical task.

## Older-history tools

Controller-backed managed execution supplies `search_conversation_history` and
`read_conversation_history` alongside the other task tools. Polling compatibility
workers without the control gateway remain tool-free. The backend binds workspace,
agent, requester and conversation from the authenticated running task. Supplying a
different scope in tool arguments is rejected. Each call rechecks the current
claim/lease, membership, agent participation/grant and source authority.

Search accepts a literal query of up to 256 bytes and an optional `before` cursor.
It scans at most 256 eligible messages, returning at most 20 attributed excerpts
of 600 characters each. Matching is case-insensitive, not semantic; `%` and `_`
are ordinary characters. An empty query browses history. Returned messages are in
chronological order. Follow `next_before` until null; an empty page with a cursor
means older evidence remains unsearched. The requested cursor cannot extend past
the original task's reply sequence, so later unrelated messages do not enter it.

Read accepts an eligible message UUID and optional zero-based character offset.
It returns up to 4,000 Unicode characters and `next_offset` for the next chunk.
Offsets are not UTF-8 byte positions. Foreign, missing and excluded IDs return the
same unavailable result. Returned author names are current profile values, while
stable IDs and stored message timestamps retain attribution.

Both tools preserve the recent-context selection policy: managed profiles use
their current conversation, including its threads, with the current since-joining
restriction; revision-zero profiles retain their narrower thread selection.
Messages from another channel or private DM are never searched. A legacy channel
task lacking a recoverable thread cannot use this tool to gain a wider scope.
Reads create no jobs, action receipts or approvals. They do not send messages or
grant business-operation rights. History queries have a five-second SQL timeout.

## Compatibility and limits

No schema migration or new memory service is required. Existing queued payloads
remain unchanged. Where the task has an exact request-message link, execution can
rebuild its evidence even if the old stored prompt used flattened text. Historical
tasks without that link retain the old prompt and report unknown initiating origin;
the controller does not guess a sender from prose. Their embedded legacy history
cannot be safely reconstructed, so it does not gain the new per-message refresh
guarantee. Drain or stop such old work before relying on the new context contract.
New Library tasks now also retain the exact request-message link. Prepared worker context
uses the existing version-1 request/instructions fields; its wire schema does not
change. The legacy SQLite/local runtime is unchanged.

This increment does not add automatic memory formation, semantic retrieval,
email/WhatsApp identities or external identity linking. Existing
agent-wide maintainer notes still have their documented broader audience; they
are not automatically learned from private conversation history.

## Local verification

`cargo test --locked --lib conversation_context_tests -- --ignored` with a
disposable `RELAY_TEST_DATABASE_URL` checks same-name speakers, forged identity
text, current profile changes, source conversation isolation, agent authors,
revocation, exact retries and escaped-input bounds.

After `cargo build --locked`, `node --test tests/trusted_sender_context.mjs` uses
real pinned Goose with a synthetic local inference provider and a disposable
schema. It inspects actual model input across API restart and multiple speakers.
This validates context transport and isolation, not real-model business judgment
or hosted production readiness.

`cargo test --locked --lib conversation_history -- --ignored` covers bounded
pagination, literal matching, Unicode chunks, task cutoff, source exclusions,
current joining policy, revocation and unchanged task records. Guest forwarding
tests reject caller-selected identity/scope. `node --test
tests/conversation_history.mjs` exercises the actual pooled Goose/MCP/controller
path with older evidence, foreign IDs, restart and revocation using a local
scripted provider.

`cargo test --locked --lib request_context_tests -- --ignored` covers linked old
prompts, current attribution, unchanged durable payloads, original request cutoff,
helper versus human identity, automation origin and historical unknown origin.
The real Goose handoff journey also inspects the helper's actual model input.
