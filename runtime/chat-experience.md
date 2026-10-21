# Unified chat composition and readable responses

Channels, shared direct messages, threads and owner-private agent conversations now
use the same React composer. The private transcript retains its existing tools,
approvals, artifacts and event transport; its old DOM composer has been removed.
A React-owned island is explicitly mounted/disposed within the compatibility subtree.
Private drafts and submissions retain their existing per-agent/account/workspace
storage; shared chat retains its durable IndexedDB outbox and exact retry IDs.

## Response rendering

Both transcript renderers and pending shared messages use `web/src/markdown.ts`.
Marked parses GFM headings, lists, emphasis, quotations, tables and fenced code;
DOMPurify applies an explicit tag/attribute allowlist before HTML reaches the DOM.
Raw HTML is escaped. Model-provided images are text, not automatic remote requests;
authenticated uploaded attachments keep their existing media renderer. Links allow
HTTP(S) and mailto, with noopener/noreferrer. Code and tables scroll locally.

Dependency choice: Marked 18.1.1 (MIT) and DOMPurify 3.4.17 (Apache-2.0 OR MPL-2.0) provide one focused parser/sanitizer boundary usable in
React and the remaining DOM transcript. No editor framework is added. The real
integration tests cover formatted text, unsafe links/HTML and partial streamed input.
The combined entry bundle grows from 93.73 kB gzip to about 122 kB gzip in the local
production build; the lazy private-chat bundle remains about 18 kB gzip. This cost
includes the composer and media/mention changes, not only the two libraries.

## Mentions

Typing `@` searches eligible people and requestable agents. Arrow keys move through
suggestions; Enter/Tab selects; Escape dismisses. Enter sends, Shift+Enter inserts a
line break, and input-method composition does not submit. Selected names remain
readable in the input while canonical content stores `[@Name](mention:user:id)` or
`[@Name](mention:agent:id)`. Editing inside a selected name removes its identity;
editing around it preserves it. Draft reload and outgoing retries preserve the exact
serialized content. Names may coincide without changing the referenced identity.

Private channels suggest their people; public channels may mention workspace members
who already have access. The server validates membership on a new send within the
write transaction. Agent mentions must match the explicit target and pass existing
requester and channel checks. One agent may be requested per message; multiple human
mentions are supported. Ordinary `@text` does not dispatch work. Human mentions are
highlighted in the conversation and participate in ordinary unread/receipt behavior;
this increment does not add a separate mention inbox, email or push notifications.

## Replies and media

An agent requested in the main channel responds in the main channel timeline.
Only an explicitly threaded request creates a threaded response. Existing stored
threads remain untouched. Request context remains narrowly scoped as before; changing
reply placement does not expose unrelated threads or private-agent memory. Shared
agent names accompany authorized messages so readers can identify agents even when
they lack permission to invoke them.

Attachment and voice buttons live inside the composer. Voice recordings have a
visible elapsed timer, stop/cancel controls, automatic attachment on Stop. Cancel discards a recording. Sending still requires
an explicit Send after attachment; it is blocked during recording and upload. Failed
voice uploads retain a playable preview with Retry and Cancel controls. Media tracks, preview URLs, observers and
pending requests are released on teardown. Shared upload limits remain 10 MiB/eight
files; private uploads retain their existing 25 MiB/eight-file boundary. These are
stored audio attachments, not speech recognition.

Messages retain own-right/other-left alignment, quieter incoming surfaces and compact
consecutive sender groups. Own messages omit repeated names/avatars, with time and
accessible delivery indicators on the final line. Thread actions remain keyboard/touch
accessible as compact reply buttons. The single-row composer floats inside the work
area with an inset and soft shadow, growing only for drafts, files or recording controls. Sent/delivered/seen states remain distinct. Screens are
checked at desktop and 360px widths with local scrolling.

## Verification

Run the normal Rust/web checks, then with the isolated company server running:

```sh
./tools/company-simulation.sh test
node tools/company-simulation/chat-ui.mjs
node tools/company-simulation/messaging.mjs
```

The UI rehearsal uses real shared APIs and deterministic agents, retains its messages,
files and screenshots, and makes no LLM calls. Its private-agent response/health
endpoints are mocked in the browser to verify formatting and draft retention without
provider costs. This is not a live model/tool execution test.

Verified locally: 58 Rust library tests, 21 PostgreSQL integration tests, strict
clippy, 16 frontend test files, production build, browser-test type checking and
362 source files within 500 lines. Seven real-browser UI scenarios and eight existing
messaging recovery scenarios pass. Recovery samples showed 129–163 ms display latency
for 12 messages; this remains a small local measurement, not a production guarantee.

Dependency audit reports no advisory for the new parser/sanitizer. It reports an
existing Vite 8.3.2 development-server launch-editor advisory; that unrelated tool
upgrade is not included in this change. Normal use here serves built static assets.

## Compact design follow-up

The approved floating-composer design passes all 16 frontend test files, production
build, browser-test type checking, seven browser UI scenarios and eight recovery
scenarios. Stop-to-attach, failed-upload retry and cancellation are covered explicitly.
Layout checks cover a single-row empty composer, desktop, narrow and 320px dark/short
windows. The offscreen receipt check uses a short viewport because compact messages
can now all fit in the previous desktop viewport. No real provider calls were made.
