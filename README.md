# Ayati

A small local personal assistant: one continuous conversation, one Rust agent
harness, and a lightweight TypeScript web interface. This is the first working
foundation, not the completed product in `proj-docs/development-roadmap.md`.

## Run locally

Requirements: Linux, Rust/Cargo, Node.js 26, and Bubblewrap (`bwrap`) with
unprivileged user namespaces enabled. Tested with Rust 1.98.1, Node 26.2.0,
and Bubblewrap 0.12.0 on Arch Linux. Bubblewrap must work; execution never falls
back to an unrestricted host shell.

From the project root:

```sh
cp .env.example .env
# Edit .env locally and set FIREWORKS_API_KEY. Do not paste keys into chat.
npm ci --prefix web
npm run build --prefix web
cargo run -- serve
```

Open **http://127.0.0.1:8765**. Without a key, the interface and file storage are
available, and sending a request explains the missing configuration. Restart
after changing `.env`. The provisional model is configurable with `AYATI_MODEL`.
Account availability and live model quality still need validation with your key.

Try attaching a text file and asking:

> Read this file, create a clearer revised version, preserve my original, and
> explain what you changed. Check the revised file before finishing.

Generated files appear under **Files**. Each archived version has its own download;
original uploads are kept outside the writable shell workspace. Closing the web
page does not stop work. Keep the daemon process running; automatic startup and
restart recovery are a later increment.

```sh
cargo run -- status
cargo run -- stop
# Optional optimized binary:
cargo build --release
./target/release/ayati serve
```

Run commands from the root so the daemon finds `web/dist` and `.env`. `start` is
an alias for `serve`; it runs in the foreground. The UI's Stop button cancels
current work; the CLI's `stop` shuts down the daemon. Neither undoes completed
file operations. A local machine must stay powered on to do background work.

## Implemented

- One chat, streamed replies, working status, Stop, file attachment/downloads,
  and a compact files panel. The client uses vanilla TypeScript and CSS.
- A normalized model boundary and Fireworks/OpenAI-compatible streaming adapter.
  Other provider protocols can implement `Model`; Gemini and Anthropic adapters
  are not implemented. No local inference or separate classifier is used.
- One tool loop with sequential validated calls, corrective feedback for invalid
  arguments, complete tool-call/result pairing, and explicit failure states.
  Truncated or unfinished responses cannot dispatch tools.
- Real shell commands through Bubblewrap. Only the private workspace is writable
  persistently. System binaries are read-only; `/tmp` is private; host home,
  `/etc`, provider credentials, and networking are absent from the shell.
- SQLite history, tasks, tool attempts, model exchanges, and file references.
  Files are ordinary files, archived separately from the live workspace.
- Immediate model cancellation and shell/process-descendant termination. Waiting
  requests can be cancelled before they run.
- Local same-origin requests, private data-directory permissions, a daemon lock,
  escaped chat content, and downloads forced as attachments.

SQLite SQL stays in `src/storage`; a PostgreSQL implementation will require
schema/query changes and migration, rather than just changing a connection URL.

## Current boundaries

One request runs at a time. Additional requests are saved and queued while the
daemon remains responsive. Background jobs yielding to short new requests,
corrections applied to running work, and automatic restart/resume come next.
On restart, unfinished requests are marked interrupted; completed effects are
never blindly replayed. Recent saved exchanges inform follow-ups.

Context uses a conservative **96,000-byte** ceiling, not a claim about a model's
token capacity. Up to 12 recent complete exchanges fit within half that budget.
Current progress is saved and the request stops clearly at its context or
24-turn limit. LLM compaction, full-history retrieval, automatic memory and
durable approval/ask-resume are later increments. The UI loads the latest 200
messages and files; older records remain in SQLite.

Shell output has a 6 KB excerpt and an 8 MB log cap; omission is explicit.
The model can inspect retained output with `head`/`tail` on the returned read-only
`/artifacts` log path, without repeating a command that changed files.
Commands have a 1–120 second timeout. Downloads/uploads are limited to 25 MB.
At most 20 changed files per tool step are archived, with an omitted count.
Workspace scans are bounded to 2,000 files and depth 12. Logs are available in
tool records and through `/api/files/{id}`, and are hidden from the normal files
panel. No disk quota, CPU/memory quota or retention cleanup exists yet, so this
is a local development environment, not a hardened multi-user sandbox.

The shell has no network in this increment; package downloads, browser work,
connected apps, MCP, scheduling, recorded voice, rich document conversions and
interactive generated UI are not available yet. Booking/payment approvals will
be built before tools that can make those external actions. Never store actual
credentials in uploaded files or the agent workspace. Ordinary chat and file
contents used as context go to the configured online model provider.

## Development and checks

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
npm test --prefix web
npm run build --prefix web
cargo build --release
node tests/daemon_smoke.mjs
./scripts/check-file-lines.sh  # local ignored helper, when present
```

`npm run dev --prefix web` starts Vite with an API proxy to port 8765. Rebuild the
web client for the daemon's normal single-origin interface. Dependency lockfiles
are tracked; runtime data (`.ayati`), `.env`, build output, project reference
documents and reference repositories are ignored.

Backend tests exercise a local HTTP model fixture with **real** Bubblewrap shell
execution, downloads, history, cancellation, incomplete calls and localhost
access checks. They establish protocol/runtime behavior; they do not establish
Fireworks account access or model intelligence. DOM tests cover stream/snapshot
races and safe rendering. A graphical browser check is still needed.

## Structure and dependency choices

| Boundary | Implementation | Reason |
| --- | --- | --- |
| Daemon/API | Tokio + Axum | One process for async HTTP and the harness |
| Model protocol | reqwest + small streaming decoder | Keep provider translation out of the loop |
| Persistence | rusqlite + SQLite WAL | Local durable state without a database server |
| Execution | System Bubblewrap | Reuse Linux isolation instead of creating a sandbox |
| Web | TypeScript + Vite | No UI framework runtime for the initial chat |
| Development DOM checks | jsdom + Node test runner | Test stream races and rendering without shipping a test framework |

Versions are pinned in `Cargo.lock` and `web/package-lock.json`. The Rust
libraries, Vite and jsdom have permissive licenses; TypeScript is Apache-2.0.
Bubblewrap is a separately installed LGPL-2.1-or-later executable. License and
bundling review will be needed before distributing packaged binaries.

Read `AGENTS.md`, the local `proj-docs/README.md`, and
`proj-docs/implementation-status.md` before further building. Use selective
references under `proj-docs/proj-ref`, and preserve rationale and validation in
Git history.
