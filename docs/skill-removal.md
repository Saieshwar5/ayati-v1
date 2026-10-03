# Mac-only bundled skill exclusions

Ayati targets a hosted Linux runtime. This increment removes only five bundled
skill directories from the pinned OpenClaw source under `vendor/openclaw/skills`:

| Skill | Removed instructions |
| --- | --- |
| `apple-notes` | Apple Notes through the macOS `memo` CLI |
| `apple-reminders` | Apple Reminders through `remindctl` |
| `bear-notes` | Bear notes through `grizzly` |
| `things-mac` | Things 3 tasks and projects through the Mac CLI |
| `peekaboo` | macOS UI inspection and automation through the Peekaboo CLI |

Each directory contained one `SKILL.md` with a macOS-only platform requirement.
These files teach the agent how to use an external tool; they are not the tool's
implementation. Removing them does not uninstall external CLIs or delete data.

## Preserved behavior

General skill loading, user/workspace skills, schedules, memory, file tools,
browser automation, computer-use protocols and other bundled skills remain.
Mac/Windows service adapters, VM testing tools and native browser/worker adapters
are outside this increment. This patch does not enforce a Linux-only launcher.

Synthetic test fixtures still use some retired names to prove generic skill
discovery, allowlists, status, UI layout and historical configuration migration.
They do not load the deleted source files. Native protocol fields and upstream
documentation also remain where their owner is a separate retained capability.
No skills catalog or runtime import explicitly depended on these five files.

Existing private configuration may still name an excluded skill. It will not
restore the bundled files; operators should review such entries before upgrading.
Workspace, managed, plugin or remote skills are separate discovery sources and
can provide similarly named capabilities. This exclusion is not an installation
ban, customer permission boundary or migration of private state.

## Future upstream updates

The upstream version, commit, imported tree and original dependency lock remain
unchanged. Continue reviewed subtree upgrades using
[the manual upgrade procedure](upstream-upgrades.md).

`config/source-scope.json` records the five retired bundled source paths.
`pnpm check` rejects their reintroduction, including untracked directories or
symlinks. Run it before and after building. Bundled skills are shipped from the
package-root `skills/` directory; they are not compiled into the Gateway.

For an upstream upgrade:

1. Review incoming changes in the removed directories and shared skill owners.
2. Preserve relevant shared fixes while keeping the selected bundled skills absent.
3. Review renamed or relocated equivalents; exact-path checks do not find every
   future implementation of the same feature.
4. Verify retained skill discovery, package contents and the affected runtime flow.
5. Promote only the reviewed and tested Ayati artifact.

This reduces bundled feature and source scope. These skills were already
platform-gated on a Linux host without compatible remote Mac nodes; no runtime
RAM reduction or increase in customer capacity is established.

## Verification of this increment

- Fifteen Ayati tests pass. The actual-policy regression rejects restored bundled
  directories for all five names and permits a retained skill directory.
- Seventy-four upstream tests across five skill loading/discovery suites pass
  (36.09 seconds for the runner, including worker preparation).
- The real Ayati CLI with disposable private state reports 48 bundled skills:
  the five excluded skills are absent; `tmux`, `github` and `summarize` remain.
- An npm package file-list dry-run confirms those exclusions and retained files.
  Lifecycle scripts were skipped; this is not a release build or tarball check.
- Disposable Gateway startup/restart, web serving, authenticated health,
  workspace retention and update-policy checks pass using the existing compiled
  runtime. File-length, pin/lock, source-scope and diff checks pass.

Only raw skill files and Ayati checks/docs changed; no runtime modules changed.
A full rebuild, SDK declarations, live model/tool execution, messaging delivery
and cloud tenant isolation were not tested in this increment. Nothing is deployed.
