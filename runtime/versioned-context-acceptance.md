# Local acceptance for versioned conversation context

Verified 2026-10-26 on the sequential follow-up to merged PR #90. Read the
[behavior and limits](versioned-conversation-context.md) before interpreting these
results as product readiness. All provider/model traffic in these checks used local
fixtures. No cloud deployment, paid inference or external messaging was performed.

## Passing checks

| Check | Result |
| --- | --- |
| Standalone real PostgreSQL checks | 251 passed |
| Final focused context rerun after profile refresh fence | 22 passed |
| Saved guidance preparation regression | 1 passed |
| Rust application / guest unit checks | 104 / 9 passed |
| React/web tests | 178 passed |
| Web production build and browser-test type checking | Passed |
| Rust workspace Clippy, all targets, warnings denied | Passed |
| Restore-tool Python checks | 20 passed |
| Fixed-build workspace/controller/Goose/browser scenarios | 18 passed |
| Connector-control scenarios, including embedded credential fixture | 2 passed |
| Source size check | 867 files, none over 500 lines |
| Selected local documentation links | 115 checked, none missing |

The S3 emulator-specific unit case was not rerun: storage code was unchanged.
The separately hosted MCP fixture is excluded from the standalone database count;
it passed through `tests/shared_agent_control.mjs` with the required fixture.
Browser type checking alone is not treated as browser execution.

## Retained evidence

- `.tmp/bounded-agent-vu5etdqu`: final 18-scenario workspace run, exit 0.
- `.tmp/bounded-agent-uvyxp7cr`: final two connector-control scenarios, exit 0.
- `.tmp/independent-worker-VN9lId/versioned-context.json`: focused feature outcome.
- `.tmp/context-verified-pg.log`, `.tmp/context-frozen-focused.log`,
  `.tmp/context-frozen-guidance.log`, `.tmp/context-unit-final.log`.
- `.tmp/context-web-verified-tests.log`, `.tmp/context-web-verified.log`,
  `.tmp/context-frozen-clippy.log`, `.tmp/context-restore-tests.log`.
- `.tmp/context-cleanup.json`: owned fixture process cleanup; normal demo retained.
- [Desktop inspection](../designs/versioned-context/desktop.png) and
  [phone/dark inspection](../designs/versioned-context/phone-dark.png).
- [OpenPencil concept](../designs/versioned-context/context.fig).

Earlier failures are retained rather than counted as passes. Initial workspace
runs exposed overly broad invalidation of independent Home/question readers; the
implementation now separates content epochs from access epochs and preserves those
readers' precise existing checks. A later browser run hit an asset rebuild during
reload and showed “Build the client before opening Cuckoo.” The final suite used
fixed assets and passed all scenarios without changing those assertions.

## What this proves

Task snapshots and cited working summaries survive controller/executor restart.
Different people/conversations remain isolated. Concurrent summary writers cannot
overwrite newer task evidence. Correction, forgetting and revoked/regranted access
fence stale context; linked forgotten/corrected source messages cannot be silently
retrieved through generic history. Helper-note invalidation propagates to parents.
The final running-task correction check rejects late publication, and the browser
clears unavailable source text while unrelated Home content remains available.

This does not establish real-model summary quality, physical erasure of retained
snapshots/backups, production-scale latency/cost, full local-runtime migration,
payroll row/field authorization or live email/WhatsApp operation.
