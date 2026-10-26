# Readable saved briefs

The recovery browser check exposed a short brief whose title, purpose and heading
consumed its two-row viewport, hiding the actual finding below an inner scroll.
The existing selected Home concept shows that finding directly.

Saved and visually inspected `brief.fig` / `concept.png` in OpenPencil before code
changes. Current design document `tab-3`, page `1401989877:8`, root `1401989877:9`
records a three-row desktop brief and the existing bounded phone card. IDs are
session-specific; rediscover the document before editing.

The original `.pen` artifact was left unchanged. After the desktop application
exited, reopening that ZIP-format artifact through the MCP `.pen` reader failed;
the focused concept was saved as a separate native `.fig` document instead.

`desktop.png` and `phone-dark.png` are inspected application captures from
`.tmp/independent-worker-Zah5hb`. The connected test first failed against the old
layout, then passed short-text visibility, keyboard scrolling of long content,
details focus restoration, compact metric sizing, 1280/720/390 widths, light/dark
mode and reload without model inference. Existing Home review/source/revision
behavior also passed. No new permission, refresh or publication behavior is added.
