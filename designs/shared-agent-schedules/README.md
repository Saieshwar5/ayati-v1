# Shared agent schedules

The saved `shared-agent-schedules.pen` document was created after inspecting the
actual connected OpenPencil documents and the selected Agents design's Automation
page. Existing documents were preserved. The new desktop/phone concepts were
saved and visually inspected before application UI changes.

OpenPencil's PNG exporter reported `No visible nodes to export` for this document.
Its SVG exporter succeeded; `desktop-concept.svg` and `phone-concept.svg` are
actual exported vector nodes. Their PNG counterparts were rendered locally with
librsvg and inspected, with long review text explicitly wrapped in the design.
They are concepts rather than screenshots of an implemented feature.

The [runtime contract](../../runtime/shared-agent-schedules.md) describes current
scope and acceptance. The four `schedule-*.png` / `schedules-blocked-desktop.png` captures show the
implemented local review, waiting run, phone/dark resume and restored-but-blocked
routines. Synthetic fixture content is used; the loaded review and 320×600
dialog bounds were checked in the final browser journey. Calendar/weekday rules and external-event triggers from the
older broad concept are separate work; the current increment exposes supported
one-time and elapsed fixed-interval scheduling only.
