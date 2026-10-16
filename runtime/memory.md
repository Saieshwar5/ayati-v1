# Scoped sourced memory

This is the first local L5 increment. Goose 1.54.0 retains working-context
compaction; Ayati owns durable facts, source validation, retrieval scope,
corrections and forgetting. Execution status, approvals and action receipts
remain in their existing authoritative tables. A remembered preference never
grants tool access or changes business policy.

## Enable

Set `AYATI_MEMORY=1` with `AYATI_RUNTIME=goose`, then restart Ayati. It is off by
default and is unavailable in the prototype. Changing the configured tools cannot
reuse an old saved Goose session: use a fresh `AYATI_DATA_DIR` for this proof,
leaving the original directory intact. SQLite databases migrate transactionally
to schema 5; older binaries refuse the newer schema. Back up before upgrading
important state; source publication is not deployment.

Host execution supports adapter tests. For VM execution, build the current guest
and prepare a **new** directory; earlier VMs are preserved:

```sh
RUSTFLAGS='-C target-feature=+crt-static' cargo build --locked -p ayati-guest --target x86_64-unknown-linux-gnu --release -j 2
/usr/bin/python3 tools/prepare-local-vm.py --directory .ayati-local/vms/memory-agent
```

Point `AYATI_VM_MANIFEST` at the new manifest, set `AYATI_EXECUTION=vm`, and keep
`AYATI_MEMORY=1`. The same preparation supports a `--browser` guest, but simultaneous browser/memory
operation requires its own integration acceptance. An old bridge cannot accept the memory capability; there is no host fallback.
The host database and source history never enter the guest workspace.

## Sources and tools

The `ayati_memory` stdio MCP extension exposes three typed tools:

- `memory_search(query)`: retrieve at most eight active facts within an 8,000-byte
  serialized-record budget. An empty query returns recent facts.
- `memory_source()`: obtain the current owner message's immutable ID, text and date.
- `memory_remember(key, content, source_id, quote, replaces?)`: save a useful fact,
  or correct one using its current ID as `replaces`. The exact quote must appear
  in this run's owner message. Previous message IDs, foreign sources, event and
  scheduled payloads, tool output, files and browser pages are not writable sources.

Forgetting is owner-only through the Memory panel/API; there is no agent forget tool.
Each MCP call uses the existing owner Allow once / Deny flow. This increment
supports assisted learning from owner messages, not unattended ingestion. The
model supplies the proposed fact, so provenance proves where evidence came from,
not that its interpretation is true. Review the proposed fact and quote. Never
store passwords, tokens or other secrets; there is no comprehensive secret classifier.

Keys are normalized to lowercase ASCII identifiers of up to 80 bytes. Content
and quotes are bounded to 2,000 bytes each. Source messages follow the existing
16,000-byte intake limit; the MCP transport also bounds its reply. Oversized or
invalid exchanges fail without automatic retries. Inspect current facts before
retrying a write whose response was lost.

The trusted dispatcher binds a short-lived, random run capability to its active
claim and conversation. Tool arguments cannot choose business, agent or customer
IDs. Stopped, cancelled, inactive and previous runs are denied. In a VM, memory
RPC uses the existing restricted model-gateway forwarding path with a separate
capability; it does not use the provider API key. Host tests use a temporary
loopback listener. The capability is inherited through the controlled process
environment, not persisted extension arguments, and is invalidated after the turn.

## Owner controls and correction

Open **Memory** in the local web app to search, inspect source quotes/dates, add
an owner note, correct a fact or forget it. Owner notes become immutable sources.
The local API is `GET /api/memory?query=...`, `POST /api/memory` with
`{key,content,replaces?}`, and `POST /api/memory/{id}/forget` with `{}`.
These use the same loopback Host/origin protections as other owner operations,
with `Cache-Control: no-store`; this is not production authentication.

Updates require the exact current ID. A stale correction rolls back without
changing memory. Superseded revisions retain their source chain but disappear
from retrieval. Owner writes stop an active task and wait for it to unwind.
Corrections and forgetting remove the saved runtime-session mapping; the next
turn creates a fresh Goose session rather than reload stale context. Memory-enabled
runtime processes end after each turn so an in-memory session cannot bypass this.

**Send a follow-up message to continue after editing.** Original chat remains
inspectable, but the discarded session's working context is not reconstructed
from that history. This conservative reset also loses unrelated working context;
precise selective context removal is not implemented.

## Forgetting and scope

Forgetting withdraws the fact's revisions and evidence sources, and other facts
based on the same evidence. It blocks the stable key and suppressed source IDs
from learning again. This intentionally favors avoiding resurrection over keeping
unrelated facts quoted from the same message. There is no automatic history import
or restore-forgotten operation in this increment. Original messages, source text,
old Goose sessions and audit rows are retained privately; forgetting is retrieval
suppression, not secure erasure or backup deletion. A deliberately supplied new
note under another key is new evidence; semantic paraphrase detection is not claimed.

Scope is currently **conversation-specific**, narrower than agent-specific memory.
Tests construct separate businesses, agents and customer conversations to verify
storage denials. The product still routes only its local owner conversation;
customer authentication, shared knowledge grants and multi-agent setup come later.
There is no broad Goose recall extension or cross-conversation memory tool.

SQLite FTS5 from the existing bundled rusqlite dependency indexes active keys and
content. Migration creates the actual virtual table and fails if unavailable.
Queries become quoted text terms, not executable FTS syntax; scope filtering is
mandatory. Search ranks lexical matches, not truth or authority. No new dependency,
embedding model or vector service was added. Expiry rules, semantic matching,
storage quotas and owner pagination remain later work.

## Validation and observations

Storage checks cover migrations, exact evidence, stale corrections, source/key
suppression, restart, bounded retrieval and denied foreign business/agent/customer
access. Real Goose/MCP tests exercise approval, source learning, denied sources,
correction/forget session invalidation, owner interruption, web controls and restart.
A host test lowers Goose's context limit through a disposable config and verifies
that its real compactor requests a summary, uses it, then retrieves scoped memory.
The summary and model choices are scripted; this proves the integration, not live
model reasoning quality. One fixture observed five provider calls, including two
compaction calls, and a 421-byte final retrieval response. These are fixture
observations, not measured token savings or production cost. No paid inference
or live-account action is needed for these checks.
