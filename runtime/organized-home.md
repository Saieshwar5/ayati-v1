> **Current layout — 2026-10-20:** Follow the [Relay design system](design-system.md)
> for appearance, navigation and page hierarchy. The older visual descriptions
> below are historical where they conflict: Home has permanent clock/notifications
> and custom panels; Chat owns conversations; Agents manages configuration. The
> quiet rail stays outside one rounded white/theme frame. Feature contracts and
> explicitly unimplemented capabilities below are not changed by this UI update.

# Organized Floating Canvas

The approved dummy-data prototype is now implemented with real saved panels and
agent work. No sample sales, stock, appointments or notifications are seeded.

## Business clock

Home's location control saves a location label, IANA timezone and 12/24-hour format
in the platform SQLite database. The same choice controls Home dates, panel update
times and schedule entry/display. Timezone names are checked against the Linux
zoneinfo database; the frontend previews the selected zone with Intl. Before a
location is saved, the browser timezone is clearly labeled Device time.

New one-time/fixed-interval schedules convert wall-clock entry to an absolute UTC
instant. Nonexistent and repeated daylight-saving times are rejected rather than
silently shifted. Existing instants and intervals never change when the location
changes. Calendar daily/weekday recurrence is still unsupported: an elapsed
24-hour interval can shift local time across daylight saving. The agent receives
the configured business timezone and this limitation in each turn's context.

GET/POST /api/home/preferences uses the existing local same-origin owner boundary.
Agents have no tool to modify the clock or owner layout. This requires Goose,
like the platform services it accompanies. No new dependency is needed.

## Fixed panels and placement

New panel writes and revisions contain exactly one supported section:

| Template | Section | Content limits |
| --- | --- | --- |
| Metric | metric | Label 80 bytes, value 40 bytes |
| List | table | 3 columns, 8 rows, 160 bytes per cell |
| Chart | chart | 12 points, 40 bytes per label |
| Agenda | table | Same bounds as List |
| Brief | text | Heading 100 bytes, body 800 bytes |

The optional template field distinguishes Agenda from List; old guest schemas can
omit it and infer the template from their one section. Existing multi-section
saved panels remain readable. Their next revision must adopt a supported template
and requires review rather than silently changing an active view. Automatic
updates preserve template, title, purpose, ownership and granted source scope.
Publication and refresh authorization remain separate from schedule creation.

The responsive grid uses compact Metric cards and standard/wide content panels.
No arbitrary pixel dimensions, executable markup or agent-controlled position is
accepted. Arrange supports dragging plus Top, Move earlier and Move later for
keyboard/touch use. Wide is unavailable on Metric. Placement and widths persist
in this browser's existing ayati.home.layout key and survive content updates.
The grid follows priority order rather than filling gaps ahead of important cards.
Cross-device layout synchronization remains out of scope. Long content stays
within a bounded snapshot; review and maintenance controls expand on demand.

## Inbox and composer

Home shows a compact attention count. Its drawer groups Decisions, Problems and
Updates by agent. Exact action details and individual approvals remain intact;
there is no bulk approve. Lost/stale connectivity still disables decisions.

Add panel chooses an existing agent and opens Creating a panel mode. The composer
shows a highlighted badge and template selector while preserving any draft. It
never inserts starter prose. Create panel is also available directly in chat;
normal text requests still work. Exit mode preserves the draft. The mode and
selected template survive reload in the URL. Sending attaches explicit request
context to the submitted message; it grants no tools or execution authority.
App creation and contextual editing use the same visible mode treatment.

## Validation

Targeted Rust tests cover template bounds, source/approval ownership, clock
persistence, schedule history and durable intake. Browser journeys cover creation
without automatic submission, drafts, publication approval/denial, arrangement,
reload/restart, timezone settings, stale/offline approvals and responsive layouts.
Timezone tests include New York gaps/folds and Lord Howe's half-hour transition.
These use an isolated scripted provider; they do not prove live model quality or
upgrade existing VM images. New guest builds advertise template selection; older
guests can use inferred templates and retain their supported publication flow.
