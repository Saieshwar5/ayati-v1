# Desktop application removal

Ayati is a cloud product accessed through the web and messaging channels. This
change removes installed desktop applications from the pinned OpenClaw source.
It does not implement customer signup, a new web UI or multi-customer hosting.

## Removed

- `apps/macos`: the native Mac app and its app-specific CLI/package code.
- `apps/linux`: the Tauri desktop app, including its cross-platform desktop shell.
- `apps/macos-mlx-tts`: the Mac app's local speech helper.
- Exclusive app bundle, installer, signing/notarization, staging, Sparkle,
  appcast and Linux desktop updater tooling and their dedicated tests.
- The Mac elevation installer, which installed the deleted `OpenClaw.app`.
- Mac-only Swift generators and source-parity assertions. The shared TypeScript
  host environment security policy and its runtime tests remain.
- Desktop-shell E2E tests that depended on the deleted Tauri UI.

The source version, Node/pnpm pins and upstream dependency lock are unchanged.
This is an Ayati patch on the imported source, not a new upstream version.

## Preserved dependencies

`apps/ios`, `apps/android`, `apps/shared` and `apps/swabble` remain. Shared Swift
protocol/chat libraries, mobile voice support and the web Control UI still use
those boundaries. The iOS Gateway WebSocket test helper now belongs to
`apps/ios/Tests`; its Xcode project no longer reads the deleted Mac tree.

The release-version test fixture moved from the Tauri package to
`test/fixtures/release-version-cases.json`. Apple device identifier data used by
the web UI keeps its original MIT notice and license under
`ui/public/licenses/apple-device-identifiers`; Vite copies these public assets
into the served UI bundle.

Localization and native catalog generators own retained mobile/shared inputs
explicitly. Missing required mobile inputs still fail; generators cannot create
the retired Mac resource directory as a side effect. Native source inventory and
the derived iOS catalog were refreshed through their canonical generators.

Generic OS support, command-line node workers, browser tools, cloud browser
viewers and the web/mobile native bridge remain. Their names may contain
"Mac", "desktop" or "native" without belonging to an installed desktop app.
Removing these runtime capabilities requires a separate feature decision.

## Update protection

`pnpm check` now verifies the original import/pin/update policy and the source
scope. The scope guard inspects the working tree, including untracked files,
so an upstream merge or generator that restores retired tooling fails the check.

`config/source-scope.json` records allowed app directories, required retained
files, retired paths and commands. A new application directory must be reviewed
deliberately. Checks run before and after the CI build. The artifact scan catches
native GUI bundles/installers in emitted `dist` output; it does not audit frozen
dependency contents or establish a security boundary against malicious code.

After a manual subtree upgrade:

1. Inspect new app directories, packaging tools, package commands and generators.
2. Resolve conflicts while preserving needed runtime/mobile changes and desktop
   exclusion. Review upstream changes inside deleted paths before discarding them;
   useful fixes may now belong to a shared retained owner.
3. Regenerate affected source-owned catalogs, then run the pin, scope, affected
   upstream tests, build and Gateway checks.
4. Inspect new output paths and installer names. Expand the guard when a reviewed
   upstream change introduces another desktop entry point.
5. Promote the tested Ayati artifact through the manual release process.

Keep the original subtree import metadata. Do not replace its recorded upstream
tree hash with the patched Ayati tree hash. Restoring deleted desktop code is a
reviewed product decision, never a routine conflict-resolution shortcut.

## Verification boundaries

The root `.github/workflows/openclaw-baseline.yml` is Ayati's active CI workflow.
Vendored upstream workflows and release orchestration remain reference material;
they are not an Ayati release/deployment pipeline. Some retain historical desktop
job/artifact references. Linux desktop dispatch is explicitly rejected before any
remote operation. Desktop closeout is retired while its generic evidence checksum
operation remains. Generic core finalization was extracted from the deleted Linux
channel owner with its authorization and readback checks preserved. Do not run
upstream's publishing workflows to release Ayati.
Ayati's build/start/check entry points are documented in the root README.

This pruning reduces source and desktop maintenance scope. Desktop apps were
already outside the server build and pnpm workspace, so it does not establish a
runtime RAM reduction. Retaining mobile source does not prove an iOS/Android
build: native toolchains/device checks are separate gates.

The Gateway smoke check uses disposable private state, serves the web UI,
authenticates health connections and restarts with the same credentials and
workspace marker. It does not prove model/tool execution, conversation recovery,
browser handoff, messaging delivery or tenant isolation.
