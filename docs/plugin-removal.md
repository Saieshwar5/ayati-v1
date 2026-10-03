# Optional plugin exclusions

Ayati's hosted product does not need these five integrations. Their source
packages and dedicated tests are removed from the pinned OpenClaw subtree.
No other extension is selected for removal by this increment.

| Plugin | Removed behavior | Effect on Ayati |
| --- | --- | --- |
| `apple-fm` | Apple Foundation Models inference on a supported Mac | No Mac on-device inference provider; other model providers remain. |
| `facetime` | Experimental native Mac FaceTime calls | No FaceTime calling; generic speech, voice input and other calling plugins remain. |
| `imessage` | iMessage/SMS through a signed-in Mac and its `imsg` bridge | No bundled iMessage setup or messaging; Telegram/WhatsApp packages remain. |
| `migrate-claude` | Importing Claude Code/Desktop instructions, skills and configuration | No Claude installation importer; Claude model support remains. |
| `migrate-hermes` | Importing Hermes configuration, memories and supported credentials | No Hermes installation importer; Ayati website onboarding remains future work. |

## Related cleanup

Explicit build/test inventories no longer name the deleted source owners. Channel
config metadata, aliases and the generated official channel catalog are regenerated
from retained plugins. FaceTime is removed from the fallback plugin catalog. The
web channel gallery no longer recommends iMessage.

The unused lazy `IMessageConfigSchema` export is retired from
`openclaw/plugin-sdk/bundled-channel-config-schema`; it would otherwise try to load
the deleted plugin. This is an intentional difference from the upstream SDK.
Third-party integrations using that export need review before installation.
Shared config types and generic historical channel identities remain so this
cleanup does not rewrite stored conversations or unrelated data contracts.

Shared speech/model tools, the general migration framework, other importers and
dependencies used by retained packages remain. Existing deployments configured
with a removed plugin must remove or replace that configuration before upgrading;
this source patch does not migrate their private state or credentials.

## Future upstream updates

The upstream version, commit, imported tree identity and original dependency lock
remain pinned. Frozen installation still uses the original lock; obsolete importer
records are retained as baseline provenance. pnpm can recreate dependency-only
folders for these obsolete records. `pnpm setup:runtime` removes only those five
link-only folders after installation and refuses cleanup if source files or
substituted directories are present. Use this root setup command rather than
installing directly inside the subtree.
This is not a dependency-pruning or performance milestone.

Continue manual subtree upgrades on a review branch. Accept fixes to retained
OpenClaw code, keep the five packages excluded, and review new cross-package
dependencies before resolving conflicts. Never accept an incoming deleted package
merely because Git reports a modify/delete conflict.

`pnpm check` rejects these packages' source directories, `dist/extensions` and
`dist-runtime/extensions` output directories, and fallback catalog entries. Run
it before and after the build. The guard is a regression check for the shipped
tree; it is not a customer authorization or external-plugin installation boundary.

Regenerate channel metadata with `pnpm --dir vendor/openclaw config:channels:gen`
and the channel catalog with `pnpm --dir vendor/openclaw channels:catalog:gen` when
an upgrade changes those inputs. Run the matching checks, affected contracts,
the supported build and disposable Gateway smoke check before promoting an update.

OpenClaw fixes to these removed integrations can remain in upstream. Reintroducing
one later is a separate product decision and integration review. Removing them
does not prevent upgrades to the remaining OpenClaw engine.

## Verification of this increment

- Frozen install and repeat setup pass with the original lock unchanged.
- Fourteen Ayati tests, affected upstream tooling suites, 308 focused core/contract
  tests and 38 channel-view tests pass. The mocked channel-page screenshot was
  inspected; it is UI fixture evidence, not a live messaging check.
- Channel metadata, channel catalog, plugin inventory and retained native catalog
  generation checks pass. The real exclusion policy rejects all five packages
  and their 15 source/output paths in disposable fixtures.
- Runtime, retained plugins and Control UI build pass with the supported
  declaration-skipping profile and an explicit 4352 MB build heap. The local
  private-QA build supplied registry test prerequisites; it is not release evidence.
- Disposable Gateway startup/restart, web serving, authenticated health, workspace
  retention and disabled automatic-update checks pass. File-length and diff checks pass.

Full SDK declarations, real model/tool or messaging execution, native device builds
and cloud tenant isolation are not validated by this increment. Nothing is deployed.
