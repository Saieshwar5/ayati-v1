# General Home panel UI

This increment follows the accepted [OpenPencil library](../designs/general-panel-library/README.md).
The platform provides reusable visual families; business purposes are configurations.
Sales, return rate and visitors can all use Chart / Line with different data and units.

## Families and compatibility

Every new draft/revision contains exactly one data section.

| Canonical template | Section kind | Presentation |
| --- | --- | --- |
| Metric | `metric`, `metrics` | One value or 1–4 identified measures; optional numeric comparison |
| Chart | `chart` | Line, bar, area, stacked bar, donut; platform-controlled colors |
| Table | `table` | 1–3 columns, 0–8 rows; optional text/number column types and local sorting |
| Collection | `collection` | 0–8 identified items with descriptions and optional status |
| Progress | `progress` | 1–4 identified measures against approved targets/ranges |
| Timeline | `timeline` | 0–8 timestamped events, newest first |
| Text / summary | `text` | Heading and plain text, preserving line breaks |

Canonical template values are lowercase. Existing `list`/`agenda` alias Table;
`brief` aliases Text. Old metric values remain strings and charts without a variant
retain Line. Existing multi-section snapshots remain readable; revisions require
one section. Optional new fields are omitted when serializing legacy sections.

Clock/location, Notifications, personal ownership, source guards, exact review,
placement and maintenance agreements are reused. Choosing a type starts no work.
The explicit agent-request form offers seven types and five chart styles. The
adopted private-agent composer offers the same choices and maps legacy template
URLs to canonical families. A selection is a request preference, not authority.

## Bounded data contract

Rust validation is authoritative. The whole payload remains at most 12,000 bytes.
Text limits are UTF-8 bytes, not character counts. Stable item IDs are unique,
nonempty and at most 80 bytes. Unsupported fields, including item URLs and actions,
are rejected.

- `metric`: `label` (80 bytes), `value` string (40 bytes), optional `comparison`.
  Comparison contains `label` (100 bytes), finite numeric `value`, optional `unit`
  (40 bytes), and `favorable`: `increase`, `decrease` or default `neutral`. The
  sign comes from the value; good direction determines its interpretation.
  `metrics` uses `items` with stable `id`, label, value and optional comparison.
- `table`: `columns` strings (60 bytes), matching string `rows` (160 bytes/cell),
  optional `column_types` matching the column count. Number cells are empty or
  finite unformatted numeric strings. Sorting is stable, local and retained during
  refresh. This does not add server pagination or record mutations.
- `chart`: `title` (100 bytes), optional `unit` (40 bytes), `variant` default Line,
  and either `points` or `series`. Points contain `label` (40 bytes) and finite
  numeric `value`; at most 12 points/series and four series. Series require unique
  `name` (60 bytes) and identical ordered category labels. Donut accepts one
  non-negative series; stacked bars require non-negative data. Totals remain finite.
  An accessible Chart values table retains full labels and values.
- `collection`: `items` with `id`, `title` (100 bytes), optional `description`
  (240 bytes) and optional neutral `status` (40 bytes).
- `progress`: `items` with `id`, `label` (80 bytes), finite `actual`/`target`, optional
  `minimum` (default zero) and optional `unit` (40 bytes). Target exceeds minimum.
  Percentages describe position within the range. Bars clamp to 0–100% while the
  actual/target values remain visible and honest outside that range.
- `timeline`: `events` with `id`, valid JavaScript-range Unix millisecond `at_ms`,
  `label` (100 bytes), optional `description` (240 bytes). Native `time` retains
  the ISO instant; display uses the viewer's locale/timezone.
- `text`: `heading` (100 bytes) and `body` (800 bytes), escaped rather than executable.

Empty charts/tables/collections/timelines show explicit empty states. A zero-total
donut says no positive values can be plotted. These represent successful saved
results; failed reads still cannot become invented zeros or empty business results.

## Maintenance and authority

The [existing agreement](home-panel-maintenance.md) controls when and from which
exact reviewed reads a panel refreshes. Rendering, sorting, chart geometry and
progress calculation use trusted client code without a model. Source retrieval
and preparation retain the existing responsible-agent path; there is no new query
engine, integration or scheduler authority.

The shared presentation-shape check allows changes to metric/comparison values,
chart points, table rows, collection items, timeline events and progress actuals.
It preserves family, title/purpose, metric labels/identities, comparison period/unit/
good direction, chart title/unit/variant/series identities, table columns/types and
progress labels/identities/targets/ranges/units. Changing those requires a new
definition review and renewed maintenance authority. Both local-owner automatic
updates and managed personal snapshots use this boundary.

Source revocation hides all affected families through the existing source-safe
projection/invalidation. No new HTML, scripts, remote media, arbitrary links or
business actions are accepted. Source details retain the authorized conversation reader.

## Delivery and limits

No dependency, migration, cloud service or paid inference is added. Existing
personal maintenance still requires schema 33. Deploy updated controller, guest
bridge and web assets together before writing new families. Old snapshots remain
readable; older controllers cannot maintain new section kinds after a code rollback.

Item actions, resource calendars, boards, maps, unbounded tables, independent query/
aggregation setup and event refresh remain separate work. The catalog is not seeded
Home content or evidence of a live business integration. Verification is retained
under `.tmp/general-home-panels/`; examples run only in disposable fixtures.

## Local verification

188 workspace Rust tests (109 root unit, nine guest, 70 integration), 269 PostgreSQL
regressions, the companion authenticated gateway fixture and 193 client tests pass.
The final presentation-compatibility follow-up also passes all 17 Home PostgreSQL
cases. Clippy with warnings denied, formatting, production web build, browser-test
types, source limits and whitespace checks pass.

Five real-Goose/scripted-provider process/browser journeys cover the managed and
adopted Home paths, exact review/publication, current type/style choices, retained
drafts, numeric sorting, scheduled updates, restart, source revocation and member/
Owner privacy. Four affected journeys were rerun after the final chart-readability
adjustment; the case counts overlap rather than adding more unique coverage.
320/390 px, desktop and dark-theme checks assert full plot/legend/comparison bounds.
Exact chart values retain numeric precision; small nonzero values do not format as zero.
See [captures and check manifest](../designs/general-panel-library/README.md).
