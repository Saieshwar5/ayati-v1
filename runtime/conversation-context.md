# Trusted requester and attributed conversation context

Managed PostgreSQL Chat prepares a fresh, bounded Goose context for each attempt.
The controller identifies the human requesting principal separately from message
content. An authenticated task, not a name in a prompt, determines the principal.

## Current behavior

Newly queued messages store an attributed JSON transcript: stable message and
author IDs, human/agent kind, sequence, timestamp, thread, content and display name.
The explicit request message ID identifies which message the task answers. A
display name is a profile value observed at intake; it is neither a historical
identity guarantee nor an authorization claim. Distinct people may share a name.
Other speakers' email addresses are not included in the transcript.

Before each managed execution, current stored requester ID, display name and
email are assembled with workspace, agent, task and conversation identifiers.
The exact running claim, current membership, conversation access, agent grant and
source rights must still pass. Current identity can reflect a rename while the
saved transcript retains its earlier profile snapshot. The requester is the human
responsible for the task, including background and delegated work; this does not
claim that they personally typed an automation's message.

Profile values, conversation names and bodies remain untrusted data. Identity
context grants no action, approval, source access or verified external-customer
identity. The model is instructed not to repeat private contact details without a
relevant authorized purpose; actual business operations still pass server checks.
This improves attribution, not a guarantee against every model disclosure error.

Managed recent history remains scoped to the current conversation, at most 60
messages. Revision-zero profiles retain their existing 30-message/thread selection.
The 14,000-byte budget now counts serialized attribution and escaped content. The
latest explicit request remains intact if it alone exceeds that budget, bounded
by the existing 12,000-byte input limit and prepared-context limit. Older messages
are removed first. The existing since-joining and event/handoff exclusions remain.

## Compatibility and limits

No schema migration or new memory service is required. Existing queued prompts
remain unchanged rather than inventing missing historical attribution. Managed
execution still supplies their current requester metadata. Prepared worker context
uses the existing version-1 request/instructions fields; its wire schema does not
change. The legacy SQLite/local runtime is unchanged.

This increment does not add automatic memory formation, semantic retrieval, older
history tools, email/WhatsApp identities or external identity linking. Existing
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
