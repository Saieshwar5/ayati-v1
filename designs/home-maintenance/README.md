# Personal Home panel updates

The editable [OpenPencil document](home-maintenance.fig) was created before
application UI edits on 27 October 2026. Its two frames explore the desktop update
agreement and phone panel health. Concept values are illustrative.

The application keeps the existing personal Home and standard clock/Notifications
structure. Panel updates name the maintaining agent, expose exact reviewed reads,
instructions, an optional elapsed interval and maximum age. Routine snapshots do
not change the panel definition or personal arrangement. Full source-read time and
next check remain in details; the card has concise status and Update now.

Actual application captures from the final real-Goose/scripted-provider journey:

- [Desktop agreement](agreement-desktop.png)
- [Desktop Home](home-desktop.png)
- [Phone](phone.png)
- [320px phone, dark theme](phone-dark.png)

The journey verifies explicit member-owned setup, exact reads, manual/scheduled
publication, keeping an unsaved agreement draft through a data update, second-device
reading, another Owner's denial, controller restart, pause, source revocation and
keyboard focus return. It records 11 scripted model requests, three local source
reads and zero external writes. Full metric values remain visible at 320px.

Reproduce with `RELAY_TEST_DATABASE_URL=... node --test tests/home_panel_maintenance.mjs`
after building Rust/client and installing the pinned Goose artifacts. The database
must be loopback and named `relay_simulation_*`. Evidence is retained locally in
`.tmp/home-maintenance/verification.json` and `.tmp/independent-worker-a5iWr3/`.
See the [runtime contract](../../runtime/home-panel-maintenance.md) for boundaries,
schema 33, further checks and remaining work.
