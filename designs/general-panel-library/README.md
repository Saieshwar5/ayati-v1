# General Home panel library

This is the design source for **general reusable panel UI**: business purposes are configurations of shared components. It supersedes the earlier purpose-specific exploration.

Open [general-panel-library.fig](general-panel-library.fig) in OpenPencil. The document contains editable text, frames, shapes and vectors. Artwork retains design-proposal labels and illustrative data. The subsequent application increment is described in the [general Home contract](../../runtime/general-home-panels.md); use that contract for implemented bounds and remaining interactions.

## Pages

| OpenPencil page | Preview | Contents |
| --- | --- | --- |
| 00 · General panel library | [Overview](overview.png) | Seven general families: Metric, Chart, Table, Collection, Progress, Timeline, Text / summary |
| 01 · Chart variants | [Chart variants](chart-variants.png) | Line, bar, area, stacked bar, donut; shared chart configuration |
| 02 · Panel variants | [Other panel variants](panel-variants.png) | Metric grouping and comparison, typed tables, collections, progress, timeline, supported text blocks |
| 03 · Same UI, different data | [Reuse examples](same-ui-different-data.png) | One Chart / Line renderer configured for sales, return rate and visitors |

## Component model

- **Type:** a predictable visual structure and data contract, such as Chart or Table.
- **Variant:** a supported presentation within a type, such as Chart / Line.
- **Configuration:** source, fields, calculation, units, filters, labels, time range and refresh policy for a particular panel.
- **Instance:** the saved personal panel rendered from that configuration.

Panel type names remain general. A sales chart, return-rate chart and visitor chart are instances of the same reusable renderer. Business recipes may later provide starting configurations without becoming new platform renderer types.

Every family shares source access boundaries, checked time, health, size/placement and allowed source links. Metric comparison direction is configured: an increase can be favorable for one measure and unfavorable for another. Chart axes retain numeric values; units and precision are formatting settings. Ratios require explicit numerator and denominator definitions.

The library follows the typography, neutral surfaces and selective Home-panel borders in [the design system](../../runtime/design-system.md). It is a component catalog rather than a full Home-page redesign. Existing clock and Notifications areas remain separate standard Home UI.

## Verification and files

The implemented UI was also exercised with real Goose and scripted local inference:
[desktop chart](implementation/chart-desktop.png), [320 px dark chart](implementation/chart-phone-dark.png)
and [general type chooser](implementation/type-picker.png). These are synthetic fixture
captures, not live business data. [Implementation verification](implementation-verification.json)
records the local checks and limits separately from the native design checks.

All four pages are exported directly from OpenPencil and visually reviewed. Native font and major-overlap checks are recorded in [verification.json](verification.json). These are layout checks, not functional application tests.

The `.fig` file is the editable source. Adjacent `.jsx` files are native OpenPencil construction artifacts, not application React components. `catalog-index.json` records session IDs for inspection; relist actual documents/pages before future editing because these IDs can change after restart.
