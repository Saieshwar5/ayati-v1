# Versioned managed conversation context

Managed PostgreSQL tasks now retain the first assembled input per execution claim
in schema 31. An exact start replay uses the same snapshot. A new claim assembles
fresh context. Assembly starts with a source/access generation and saves only if
that generation still matches under the workspace transaction. Credentials and
provider keys are not part of `Prepared`.

## Recorded evidence and reads

`relay_context_attempts` stores the task, claim, requester, agent, conversation,
protected prepared input and selected message/note evidence. Note IDs and revisions
are retained with the selected text; changing a live note does not rewrite an old
snapshot. The recorded input is evidence of controller preparation, not proof that
a provider received it, that the model used every source, or that an action ran.

Scoped history search/read results are appended to `relay_context_reads` in the
same serialized transaction as access validation and retrieval. A task attempt is
bounded to 100 history calls and 262,144 serialized result bytes. Other source
content and tool effects retain their existing Library/event/action/handoff
records and access paths; the context UI does not expose their raw protected data.
Initial prepared input remains bounded to 262,144 serialized bytes.

## Working summaries

The controller tool `save_working_context` accepts a bounded objective, reported
decisions, unresolved questions and corrections/conflicts. Every entry needs
message or approved-note IDs present in that attempt or its recorded history reads.
The summary is untrusted model interpretation, not a verified factual extraction,
approval, tool grant, exact operation receipt or substitute for business records.
An agent is instructed to save a summary when useful for long work. There is no
additional automatic summarization model call and no semantic-search engine.

Versions are scoped by workspace + agent + conversation + requesting human.
Participants share permitted channel messages; one participant's personalized
summary is not silently loaded into another person's request. Summaries use an
expected revision and monotonic task-message cutoff. Exact retries are idempotent;
stale writers and older tasks cannot overwrite newer work or consume future
summaries. Evidence that fell outside recent context can be retrieved through the
existing scoped history tools before saving a new summary.

Tasks with external initial sources, business actions, questions or handoffs cannot
save generic conversation summaries. They retain the source-aware continuation and
receipt paths. This conservative boundary avoids promoting restricted tool output
into general reusable context. There is no automatic cross-channel identity merge.

## Corrections, revocation and retention

Durable scoped epochs fence snapshots, summaries, tool access, history reuse and
publication after source/access changes. Conversation-scoped note changes fence
that agent/conversation; agent-wide notes/profile changes fence the agent; channel
membership/visibility changes fence the conversation. Workspace membership and
connector/Library authority changes conservatively fence workspace context. This
can discard more summaries than strictly necessary; it never expands access.
Revoking then regranting access does not make an old context version current again.
Independent Home snapshots and question records retain their existing precise
connector/Library source checks, plus content-specific note/profile/message fences.
An unrelated connection change does not hide a separately sourced Home panel.
An invalidated question can remain inspectable under restored membership, while
its prior task context stays unavailable and its answer cannot resume the task.
Helper note invalidation propagates through the entire derived handoff tree.

An affected active task is cancelled before further controller tools/publication;
affected saved continuations are invalidated when notes change. Fresh work builds
from current sources. Summaries that become unavailable are omitted, and old
source-dependent agent answers are excluded from generic history. Protected
response readers and snapshot readers recheck current authority.

A new ordinary message describing a correction remains original evidence; it does
not magically edit a note. The agent can record the correction in a newer cited
summary. Approved-note edit/forget is the explicit deterministic invalidation path. When a
note has an exact linked source message, correction/forget also excludes that
original message from generic context/history retrieval, so it is not silently
relearned through that path. Unlinked notes cannot identify all paraphrases or
copies; full semantic forgetting is not claimed.

Forgetting hides affected text from future context and current inspection. It does
not physically erase immutable snapshots, backups, already-delivered messages or
provider logs. Full retention/erasure and the older local-runtime memory migration
remain separate work. No payroll row/field policy is inferred from conversation
membership or a summary.

## Inspection

Agents → Knowledge → Preview conversation context shows current recent evidence,
the current person's working summary, and metadata for their latest 20 attempts.
Opening one attempt requests its evidence separately; refresh/poll/focus and
identity changes clear outdated views. Historical data requires current membership,
agent/conversation access, agent participation and source permission. Workspace
ownership/maintainership does not open another person's private chat or task input.
The exact prior working-summary version and profile revision are included in task
inspection. Raw system prompts, credentials and external-source payloads are not
sent to this UI.

`GET .../agent-profiles/{agent}/context-preview?chat={chat}` returns current context
and summary/attempt metadata. Supplying both `task` and `claim` selects one exact
attempt in that same currently authorized conversation for the signed-in requester.
Unavailable versions return metadata without saved text.

See the [local acceptance record](versioned-context-acceptance.md) for retained results.

## Local verification

`cargo test --lib context_ -- --ignored` covers real PostgreSQL authorization,
version conflicts, source corrections/forgetting and stale publication. Set
`RELAY_TEST_DATABASE_URL` to a disposable database only.

`python3 tools/run-agent-load.py --suite context-versions` uses a fresh private
PostgreSQL database, bounded real Goose executor, scripted local provider and live
Chromium. It verifies history receipts, cited summary save/reuse, controller and
executor restart, cross-person denial, correction during running work, forgetting,
and desktop/phone dark-mode inspection. It does not prove real-model summary
quality, production retention, cloud cost or production capacity.
