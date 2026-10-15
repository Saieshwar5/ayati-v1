# Ayati

Ayati is a planned cloud business workspace for organizations run by one to
four people. It combines purpose-specific AI agents, custom internal business
apps, and simple owner/staff supervision through the web.

The engine will be built in Rust for Linux. Agents will have separate memories,
explicit connector grants, and their own real Linux VMs with persistent storage.
Always-on services receive work while eligible idle VMs may safely sleep.
Guided setup, understandable controls, and text/image/document/audio input are
central product requirements.

## Current status

The previous OpenClaw implementation, Node launcher, tests, configuration,
workflow, dependencies, and obsolete build output have been removed from the
active tree. The new Rust engine has not been implemented. There are currently
no supported application build or start commands.

OpenClaw remains available as a local reference under proj-docs/proj-ref/openclaw.
Git history, planning documents, utility scripts, private state, and existing
backups are preserved.

## Plans

Start with the local [planning index](proj-docs/README.md),
[cloud engine plan](proj-docs/cloud-engine-plan.md),
[development roadmap](proj-docs/development-roadmap.md), and
[implementation status](proj-docs/implementation-status.md).
The [cleanup record](proj-docs/implementation-cleanup-2026-10-15.md) explains
what was removed and how to recover the old source.

Planning documents and reference projects are ignored and available in this
workspace; normal clones do not contain them. Dependency versions, measured
capacity, budget, and delivery dates remain undecided.

## Development and recovery

Follow [AGENTS.md](AGENTS.md). The local source-line checker remains available
as ./scripts/check-file-lines.sh when the ignored scripts are present.

The pre-cleanup source is preserved in Git at
4b70912baa6279ef0ee38cbe27bf20bc9b443b36. Recover it in a separate checkout
when needed. Local material that Git did not track is retained under
.tmp/implementation-before-rust-2026-10-15. Private .ayati and .ayati-openclaw
state has not been deleted or migrated.
