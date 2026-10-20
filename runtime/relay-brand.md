# Relay client identity

The public client name is Relay. Its black handoff mark is shared by the
identity screens, preview and main workspace navigation. Browser titles,
accessible labels and product copy use Relay. The built-in local guide's
legacy default name is mapped for display; custom agent names are preserved.

## Artwork

- `web/public/brand/relay-logo.svg`: outlined horizontal wordmark and symbol.
- `web/public/brand/relay-symbol.svg`: compact, scalable production symbol.
- `web/public/brand/relay-favicon.svg`: white mark on a black rounded tile.
- `web/public/brand/relay-symbol.png`: transparent generated design reference.

The production SVG geometry is a simplified redraw of the approved handoff
direction. The wordmark uses outlined Liberation Sans Bold with adjusted spacing;
its font license is included beside the assets. No runtime font download is needed.
`web/src/brand.ts` and `brand.css` share the artwork across the client. Black is
the primary treatment; dark surfaces use white for contrast. Keep clear space
around the symbol and do not stretch its aspect ratio.

## Compatibility and scope

Backend executable/package names, environment variables, API headers, cookies,
storage keys, saved messages and existing business data retain their identifiers.
This is a client rebrand, not a data migration. WorkOS-hosted branding and email
templates are separate provider configuration and have not been changed.
This implementation does not establish trademark or domain availability.

## Design reference generation

Generated with the built-in image-generation tool using the previous concept
as the reference, with transparency enabled. Final prompt:

> Use case: logo-brand. Refine the supplied Relay concept into one permanent reusable black brand symbol. Use the small BLACK symbol at bottom of reference as the only design target. Preserve its recognizable silhouette: two separate flowing rounded baton forms with a clear white/transparent handoff channel, top form horizontal then curling down and left, bottom form curling from upper left into a lower right diagonal foot, abstract R. Make geometry optically balanced and clean, consistent curve radii, strong negative space that remains legible at 24 px. Output ONLY ONE centered solid pure black symbol on actual transparent background, ample modest padding, square canvas. No wordmark, no other icons, no background, no gradient, no shadows, no texture, no tiny details. Flat crisp vector-like edges suitable for a modern business app.

## Validation

Client production build, five existing test files, browser-test type checking,
and repository source-line limits pass. The signed-in workspace was inspected
at mobile and desktop widths. Authentication and authorization are unchanged.
