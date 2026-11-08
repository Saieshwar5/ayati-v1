# Channel-visible agent handoffs

`channel-agent-handoffs.pen` is an editable OpenPencil document created after
listing the actual connected documents and selecting its new page. Desktop and
phone concepts were saved and visually inspected before application UI changes.
Existing open documents were preserved. The selected phone frame explicitly wraps
long lines; its earlier unwrapped exploration remains in the design document.

The SVGs are actual OpenPencil exports. Concept PNGs were rendered locally from
those vectors with librsvg. The design follows the quiet grey rail and one rounded
workspace surface, with flat channel relationships and clear original/helper links.

The `handoff-*.png` captures show the actual implemented local channel wait,
independent exact approval, completed parent, phone/dark layout and source-reader
revocation using synthetic fixture data. They are separate from the concepts.
The [runtime contract](../../runtime/channel-agent-handoffs.md) describes current
permissions, bounds, lifecycle, verified scope and remaining adapters.

Since D246 (schema 100) the implementation follows the concepts' flat layout: the
sender's handoff message carries this card in the conversation where the request was
asked, followed by the helper's answer and the sender's reply, instead of a thread
under the sender's reply. `handoff-waiting-desktop.png`, `handoff-complete-desktop.png`,
`handoff-complete-phone-dark.png` and `handoff-approval-phone.png` were retaken with
that layout by `tests/agent_handoffs.mjs`; the source-reader captures are unchanged.
