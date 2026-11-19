# Cuckoo client identity

The public app and project name is **Cuckoo**. The identity is **Pop** (chosen
2026-11-19): the cuckoo-clock bird leaning out of a ring that also reads as the C
and the o, with a lowercase **cuckoo** wordmark. Use the symbol in the workspace
rail and the full lockup on access/account screens. Browser titles, accessible
labels, product copy and the permanent guide use Cuckoo.

## Artwork

- `web/public/brand/cuckoo-logo.svg`: horizontal lockup, symbol and wordmark (4.09:1).
- `web/public/brand/cuckoo-symbol.svg`: standalone bird and ring (1.16:1).
- `web/public/brand/cuckoo-favicon.svg`: white bird on a graphite rounded tile;
  also the desktop-alert icon, because it stays visible on dark notification centres.

The mark is drawn from circles: a ring (outer radius 35, inner 21.5) with a bite
cut where the head (radius 17.5, eye 5.2) sits, separated by a 5-unit gap, plus a
triangular beak. Colours are graphite `#17191D` and an amber beak `#F2A900`; the
files carry the amber, while the client's CSS masks render the whole mark black in
light mode and white in dark mode, keeping the rail quiet. The wordmark is Outfit
Bold converted to outlines (tracking −2.5%, x-height centred on the symbol, gap
0.3 em), so no font loads at runtime; Outfit is under the SIL Open Font License
(`OUTFIT-LICENSE.txt`). Preserve the aspect ratio and clear space. The old Relay
assets and their wordmark licence (`WORDMARK-LICENSE.txt`) remain as historical
compatibility files, with no current client references. The five directions
considered are on the private canvas "Cuckoo logo options" in the owner's claude.ai
artifacts.

## Compatibility

This is a product identity change. Existing `ayati` executable/package names,
`AYATI_*` and `RELAY_*` environment variables, API headers, cookies, storage keys,
database tables, data directories and existing records retain their identifiers.
The client maps Ayati/Relay to Cuckoo only for the `local-agent` built-in guide;
other user-created agent names are preserved. No data migration is needed.

WorkOS-hosted branding and email templates are separate provider configuration.
This change does not establish trademark or domain availability.

## Pop verification — 2026-11-19

Production client build and all 458 frontend tests pass. The production build was
served locally with the API stubbed, and Chromium was used to check sign-in (lockup
115×28, symbol 35×30) and the workspace rail (symbol 33×28) in light and dark, plus the rail
at 360 px. The brand files and favicon link were served correctly, with no page
errors. The real controller, WorkOS screens and desktop alerts were not exercised.

## Local verification — 2026-10-21

Production client build and all 17 frontend test files pass, along with browser
suite type checking, Rust build, focused guide test, diff and source line checks.
Live local Chromium checks covered sign-in, six workspace pages, SVG serving,
light/dark branding and 320px containment; no browser errors were observed.
The client build is served immediately after refresh. Updated Rust guide prompts
and console text take effect on the next normal restart; existing work was not
interrupted for branding. WorkOS-hosted screens were not changed or verified.
