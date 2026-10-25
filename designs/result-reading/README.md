# Saved response and file reading

The editable [OpenPencil concept](result-reader.pen) was created in a separate
document before application changes. Its [desktop](reader-concept-desktop.png)
and [phone](reader-concept-mobile.png) previews use illustrative content.

The implemented reader reuses the existing native React dialog for focus,
keyboard dismissal and nested thread behavior. Desktop supports a bounded
reading window and explicit Expand; phones use the full screen. The conversation
and draft remain behind the reader. This is the complete dialog increment, not
a claim that the concept's persistent side-pane layout has been implemented.

Retained application captures:

- [Saved streamed draft, desktop](reader-draft-desktop.png)
- [Complete response, phone/dark](reader-complete-mobile-dark.png)
- [Original image fit view](image-viewer-desktop.png)
- [Existing private runtime reader](private-reader-desktop.png)

These show disposable local fixture records. Real pinned Goose uses a scripted
local model, and file checks use actual PNG bytes. No customer data, paid
inference or cloud deployment was involved. The [runtime contract](../../runtime/result-reading.md)
describes tested behavior, limits and reproducible checks.
