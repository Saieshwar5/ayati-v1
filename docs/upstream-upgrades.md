# Manual OpenClaw upgrades

Ayati owns product releases. OpenClaw supplies upstream source. Fetching source,
merging it, publishing code and deploying a release are separate operator actions.
No bot, runtime updater or workflow automatically merges or deploys upstream.

## GitHub and remotes

`origin` is `https://github.com/Saieshwar5/ayati-v1.git`. Keep product branches,
pull requests and Ayati release tags there. OpenClaw is tracked under
`vendor/openclaw` as a squashed Git subtree. A normal clone includes the code;
there is no submodule checkout or runtime download of OpenClaw.

The initial adoption records the exact upstream commit and source tree in
`openclaw-baseline.json`. Squash imports preserve the subtree merge base and
Ayati history without publishing OpenClaw's complete historical object graph.
Do not replace the subtree with a ZIP or remove its merge metadata.

Remotes are local Git configuration and are not carried by a clone. After a
fresh clone, configure the upstream remote once:

```sh
git remote add upstream https://github.com/openclaw/openclaw.git
git remote set-url --push upstream DISABLED
git remote -v
```

If `upstream` already exists, inspect its URL first. The disabled push URL is
an accidental-push guard, not a server permission boundary. `main` tracks
`origin/main`, not `upstream/main`. Keep product work out of upstream pushes.

## Prepare an upgrade

Start with a clean working tree. Select a release, review its release/security
notes and resolve it to a full commit SHA. Replace the example placeholders:

```sh
git switch main
git pull --ff-only origin main
git switch -c upgrade/openclaw-SELECTED_VERSION
git subtree pull --prefix=vendor/openclaw --squash upstream SELECTED_COMMIT_SHA
```

This fetches and merges only on the upgrade branch. It may download upstream
history locally; that history is not made an ancestor of our product release.
Resolve conflicts against Ayati requirements, retaining needed fixes and our
intended behavior. Fetch/merge are not deployment operations.

Update `openclaw-baseline.json`: commit, version, upstream tree, dependency lock
hash and any toolchain changes. Use the selected upstream tree, not our patched
subtree's tree. Keep root `packageManager`, `.node-version` and `engines`
consistent with the reviewed toolchain. Commit deliberate lockfile changes;
do not refresh dependencies incidentally.

## Verify and promote

```sh
pnpm setup:runtime
pnpm build
pnpm check
pnpm test
pnpm test:gateway
```

Run additional checks for changed contracts and implemented Ayati features.
Core upgrade evidence includes model/tool work, files, streaming, Stop,
reconnect/restart, memory/context, permissions and installed plugin compatibility.
Add browser/channel and tenant-routing checks as those features are implemented.
Measure affected resource use against the previous baseline. A successful build
is not production readiness.

Test state migrations on a protected copy. Verify backups and restoration with
the matching old release. Reverting a Git commit or image alone cannot reverse
a storage migration or completed external effects.

Push the upgrade branch to `origin` and open an Ayati pull request. Merge after
review and checks using a merge commit so the subtree history and import
metadata remain reachable. Do not squash or rebase the adoption/upgrade PR.
Then create an Ayati release tag such as `ayati-v0.1.0`, build
one artifact, record its digest and explicitly promote that artifact. Deploy a
small test group before broader rollout. Release/deployment steps need their
own authorization; source import is not a production rollout.

## Production update boundary

The current launcher turns off automatic updates and runtime update checks;
it also removes agent gateway administration. Operator CLI/UI access is still
powerful. Hosted Ayati must enforce customer authorization and must not hand
customers Gateway administrator tokens.

Production should use a read-only application image and private writable state
mounts, with deployment credentials outside the agent workspace. Dependencies
and separate plugin packages must be locked, not installed from `latest` at
startup. Only the deployment operator replaces application code.

Customer signup, official messaging ingress, per-customer isolation, immutable
deployment packaging and multi-host operations are later milestones. This
baseline does not establish that those boundaries have been tested.

## Recovery

Use the retained previous artifact with a compatible verified state backup.
Reconcile actions that may already have completed before resuming work. Stop
rollout when behavior differs from the tested candidate. Preserve diagnostic
evidence while protecting credentials and private customer data.

The initial prototype recovery tag is `ayati-before-openclaw-2026-10-03`.
See the root README for its scope and separate local backup location.
