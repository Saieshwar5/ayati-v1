# Managed runtime prompt and host-context boundary

Managed workspace work receives controller-provided identity, instructions,
conversation evidence, approved notes and authorized source references. It must
not silently inherit the host operator's project instructions.

## Implemented selection

`runtime_config` marks shared execution internally as managed. The prepared
conversation entry point also selects managed guidance explicitly. This is not a
user prompt field or permission flag. Both controlled and tool-free shared jobs,
including tool-free samples, use concise managed workspace guidance. Local-owner
execution keeps its existing panel/App tools and permanent-guide behavior.

The managed guidance does not advertise local `panel_write`, `app_write`, automatic
publication or scheduling. Task-specific controller instructions still describe
actual tools such as `save_home_panel`, its exact human review, history, questions
and handoffs. The requester is not assumed to be the owner or approver. Guidance
is prose, never the enforcement mechanism for an action or data access.

## Host context isolation

Pinned Goose 1.54.0 normally discovers `AGENTS.md` and `.goosehints` in ancestor
project directories, in addition to its own configuration. A fresh scratch
subdirectory alone therefore does not establish a prompt boundary. The failure
was reproduced with harmless canaries in owned fixture ancestors for both the
polling and pooled paths; repository instructions also appeared in model input.

Managed and prepared Goose commands now set `CONTEXT_FILE_NAMES=[]`, disabling
that hint discovery, and point the child process's home at its own private Goose
root. `GOOSE_PATH_ROOT` is canonicalized to an absolute path because the pinned
runtime ignores relative roots. The normal environment allowlist and explicit
extension selection remain. The local owner's intentional project hints remain
available; no user/repository hint files are changed or deleted.

The relevant pinned source is `crates/goose/src/hints/load_hints.rs`
(`get_context_filenames`), `config/base.rs` (`get_param`, JSON environment parsing),
and `config/paths.rs` (absolute path-root requirement) at source commit
`cd643d2126fffab4c6a6186905211ba868d6a657`. The existing stock and preparation
artifacts were tested; this change adds no dependency or Goose binary patch.

This is prompt/configuration isolation, not an OS sandbox or a complete defense
against malicious model output. It does not erase material already delivered to
old sessions/providers. Start fresh managed work on the updated executable before
relying on this boundary. Prepared slots from an older executable retain its old
configuration; replace those executors through the normal drain/restart procedure.

## Local verification

- Runtime unit tests distinguish local, managed and prepared command environments
  and preserve local guide identity and the exact managed request.
- `tests/managed_context_isolation.mjs` uses real pinned Goose and a synthetic local
  provider. Actual model inputs exclude owned ancestor `AGENTS.md`/`.goosehints`
  canaries and repository guidance while retaining the exact controlled tool list
  or no tools for the compatibility worker. Both cases failed before the fix.
- Trusted-sender and older-history journeys inspect actual managed prompts.
  Question/reconstruction and personal Home review/source-revocation journeys pass.
  A Home assertion now waits for the denied-source projection after the UI clears
  stale data, rather than racing that legitimate empty loading state.
- Local-owner Home creation, approval and scheduled update remain exercised.

These are local transport/context checks with scripted inference. They do not
measure a real model's judgment or certify hosted isolation.
