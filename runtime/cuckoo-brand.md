# Cuckoo client identity

The public app and project name is **Cuckoo**. The approved identity pairs an
abstract cuckoo bird forming a C with a lowercase **cuckoo** wordmark. Use the
symbol in the workspace rail and the full lockup on access/account screens.
Browser titles, accessible labels, product copy and the permanent guide use Cuckoo.

## Artwork

- `web/public/brand/cuckoo-logo.svg`: approved horizontal bird and wordmark.
- `web/public/brand/cuckoo-symbol.svg`: standalone bird/C mark.
- `web/public/brand/cuckoo-favicon.svg`: white bird on a black rounded tile.

The SVG paths are traced from the approved image concept, retaining its geometry
without font or network dependencies. CSS masks render black in light mode and
white in dark mode. Preserve the aspect ratio and clear space. The old Relay
assets remain as historical compatibility files, with no current client references.
The concept was generated using the built-in image-generation tool; the approved
preview is retained locally in `proj-docs/branding/cuckoo-logo-concept-01.png`.

## Compatibility

This is a product identity change. Existing `ayati` executable/package names,
`AYATI_*` and `RELAY_*` environment variables, API headers, cookies, storage keys,
database tables, data directories and existing records retain their identifiers.
The client maps Ayati/Relay to Cuckoo only for the `local-agent` built-in guide;
other user-created agent names are preserved. No data migration is needed.

WorkOS-hosted branding and email templates are separate provider configuration.
This change does not establish trademark or domain availability.

## Local verification — 2026-10-21

Production client build and all 17 frontend test files pass, along with browser
suite type checking, Rust build, focused guide test, diff and source line checks.
Live local Chromium checks covered sign-in, six workspace pages, SVG serving,
light/dark branding and 320px containment; no browser errors were observed.
The client build is served immediately after refresh. Updated Rust guide prompts
and console text take effect on the next normal restart; existing work was not
interrupted for branding. WorkOS-hosted screens were not changed or verified.
