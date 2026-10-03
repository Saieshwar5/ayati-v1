# Parallels test entry-point cleanup

Ayati targets hosted Linux execution. This increment removes redundant developer
entry points; it does not remove all cross-platform source or change the supported
platforms of OpenClaw's aggregate npm-update validation.

## Removed

Under `vendor/openclaw`:

- `scripts/e2e/parallels-macos-smoke.sh` and `test:parallels:macos`.
- `scripts/e2e/parallels-windows-smoke.sh` and `test:parallels:windows`.
- `scripts/e2e/parallels-windows-prepare.sh` and `test:parallels:windows:prepare`.

The first two files only changed to the repository directory and launched a
TypeScript runner through Node/tsx. The third provisioned a Windows test VM,
installed prerequisites and managed reusable snapshots. No retained runner calls
the preparation tool. Its exclusive tests are removed; existing Windows smoke
checks still require a VM and baseline snapshot prepared outside this repository.

## Retained dependencies

`test:parallels:linux` and `test:parallels:npm-update` remain. The aggregate check
still supports Linux, macOS and Windows, with its existing platform selection,
fresh-install/update phases, retries, provider arguments and guest environments.
Fresh installs now launch the same TypeScript runners with the current Node
executable and `--import tsx`, from the repository directory. Logs, deadlines,
process cleanup and rerun descriptions still use the existing command owner.

The Mac/Windows TypeScript smoke runners, Mac Discord helper, Windows Git helper
and compiled Mac worker registration remain because the aggregate depends on
them. Shared Mac host transport, VM/snapshot logic, PowerShell guest support,
packaging and npm-update scripts remain too. Linux Parallels validation can run
from a Mac host, so a Mac-named helper is not necessarily removable.

This change leaves native service backends, core runtime, browser workers,
mobile sources, Windows OpenSSH testbox tooling and unrelated CI workflows alone.
Vendored upstream workflows do not run as Ayati's root GitHub workflows.

## Future upstream merges

The source-scope policy rejects restored shell files and package commands, and
requires the retained Linux/aggregate entry points and all three guest runners.
Run `pnpm check` before and after a build. This guards known paths; review new
upstream names or equivalent tools manually.

Resolve modify/delete conflicts in favor of the selected product scope. Preserve
upstream fixes in retained runners, shared dependencies and process handling.
Review the direct aggregate launch when upstream changes argument parsing,
runner ownership or startup requirements. Adapt tests and their routing instead
of restoring obsolete entry points. This cleanup does not change the pinned
OpenClaw commit, dependency lock, subtree import metadata or manual update policy.

The former mixed test file is split by responsibility into eight owning suites
and four shared support files, each below 500 lines. Changes to runners and test
support route to those suites. Future upstream changes to the old mixed file must
be reconciled into the appropriate owner; a subtree merge does not distribute
new tests across these files automatically. The 111 retained concrete cases keep
their original titles and parameter counts. Ten Windows-preparation cases are
removed; six cases verify direct fresh/fresh-target spawning on all three guests.

## Validation boundary

Local verification passed:

- 16 Ayati tests, including an actual-policy reintroduction regression that failed
  before the new exclusions were added and passes afterward.
- 445 distinct affected upstream tests across 17 suites. The first stable run
  passed 444; one new routing expectation missed an existing npm-update owner.
  After correction, both affected suites passed again (272 tests). An earlier
  run was invalidated by concurrent test-file creation and a local Python shim;
  the stable run used system Python with the pinned Node.
- Scoped type-aware lint and formatting for the changed scripts/tests.
- Runtime/plugins/UI build in the supported development profile (1m56.6s), with
  SDK declarations skipped, private QA prerequisites and explicit 4352 MB heap.
- Real local `--help` launches of the Linux/npm-update wrappers and direct
  Mac/Windows TypeScript runners; no VM operation or provisioning was performed.
- Disposable Gateway startup/restart, web serving, authenticated health,
  workspace preservation and disabled-update policy against the built runtime.
- Pin/lock, source-scope, file-length, whitespace and added-content credential
  pattern checks.

These checks do not establish a successful Parallels VM install or upgrade.
Actual VM runs, full typed builds, real model/channel execution and hosted
customer isolation require separate validation. Removing developer tooling does
not establish a reduction in Gateway memory use.
