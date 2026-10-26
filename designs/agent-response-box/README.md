# Agent response boxes — design exploration

## Selected and implemented: B, corner arrows

The user selected B. Chat now uses soft response boxes, a bounded fading preview
and an icon-only corner arrow for inline expansion. Expanded responses reverse
the arrow and also offer a top collapse control. Short replies stay naturally
sized. Check progress launchers are removed; Stop and input/approval controls stay.

Implemented for workspace conversations/threads and private-agent transcripts.
Saved reads retain source checks, frozen versions, explicit updates and exact
Copy/Markdown actions. The full reader remains available through a secondary icon.
No new model work is started by reading. Phone boxes span below the author row.

- [Actual desktop compact](app-corner-desktop.png)
- [Actual phone dark](app-corner-phone-dark.png)
- [Actual narrow phone](app-corner-narrow.png)
- [Actual expanded draft](app-corner-expanded.png)
- [Actual private inline reader](app-private-inline.png)

Validation: 183 web tests, production build, browser-test types and 882 source
limits pass. Two connected process/browser journeys pass with real pinned Goose
and scripted inference, including saved drafts, exact downloads, restart, inline
desktop/phone/keyboard behavior, threaded readers and the private runtime. No paid
inference or cloud changes. Additional connected source-revocation and Chat
question/restart/Stop journeys pass. Retained fixture evidence:
`.tmp/independent-worker-OhmvoC`, `.tmp/independent-worker-FpZP4f`,
`.tmp/independent-worker-Hf7IbB` and `.tmp/result-reading/`.

## Earlier exploration: symbol-based alternatives

Revised in live OpenPencil on 2026-10-26 after feedback requesting an integrated
symbol rather than a labelled expansion button. The earlier footer proposal below
is retained as reference and is superseded by this direction.

- [Three symbol options](04-symbol-options.png) · [SVG](04-symbol-options.svg)
- [Recommended compact desktop](05-symbol-compact.png) · [SVG](05-symbol-compact.svg)
- [Recommended expanded desktop](06-symbol-expanded.png) · [SVG](06-symbol-expanded.svg)
- [Recommended phone light/dark](07-symbol-phones.png) · [SVG](07-symbol-phones.svg)
- [Updated editable OpenPencil document](agent-response-box.pen)

**A, fade and double chevron, is recommended.** Remove the labelled footer,
divider and visible button frame. A soft text fade leads into a small bare double
chevron inside the lower edge. Downward chevrons indicate continuation; upward
chevrons collapse the expanded reply. Copy moves to the author row.

Use a maximum 224 px compact box, including the integrated disclosure area. The
lower 44 px is a generous tap target without a visible enclosing control. Keep
ordinary text selectable and links independently usable. Short replies use their
natural height and omit both fade and symbol. Retain the existing soft fill,
quiet border, rounded corners and light/dark palette. Check progress stays absent.

**B, corner arrows:** a familiar expand glyph in the bottom corner. It preserves
the surface, but can suggest full screen rather than inline expansion.

**C, folded corner:** a turned paper edge with a small chevron. More expressive,
but the fold may suggest a file. It needs a stronger discoverability check.

These are design judgments to validate with users, not measured usability results.

Tap the faded edge or symbol to expand the same saved reply inline. Reverse the
symbol to collapse. Preserve conversation scrolling, the message anchor, draft,
text selection, streaming bounds and existing source/access checks. Keep an
accessible name, expanded state, keyboard activation and visible focus even though
the resting control has no visible text or button frame. Hover or focus can show
a short hint and darken the symbol; touch must not require hover. For very long
replies, keep a collapse affordance reachable near the visible message edge.

Rediscovered document: `tab-2`. The revised active page is **02 · Symbol
exploration**, `836672655:1011`. Rediscover IDs after an app restart.

- Options: `836672655:1452`, at the top and left visible for review.
- Recommended compact: `836672655:1542`, below the options.
- Recommended expanded: `836672655:1655`, beside the compact screen.
- Phone variants: `836672655:1773`, below the desktop screens.

All four revised PNGs were exported from OpenPencil and visually inspected,
including text wrapping, expanded layout and light/dark phones. The native file
was saved successfully. These are static visual states; application code is unchanged.

## Earlier proposal: labelled footer

Created in live OpenPencil on 2026-10-26 in response to the supplied Chat screenshot.
This is a design proposal. No application code or runtime behavior changed.

- [Editable OpenPencil document](agent-response-box.pen)
- [Compact desktop](01-compact.png) · [editable SVG](01-compact.svg)
- [Expanded desktop](02-expanded.png) · [editable SVG](02-expanded.svg)
- [Phone light/dark and behavior notes](03-responsive.png) · [editable SVG](03-responsive.svg)

## Recommended appearance

Give ordinary agent prose one softly filled message box. Keep author, Agent label,
avatar and time above it. Reuse the existing semantic palette: `--soft` for the
background, `--text`/`--muted` for type and `--border` for the quiet 1 px edge.
Use 16 px corners, 20 px content padding, 16 px body type and 26 px line height.
The reading column remains at most 760 px. Match the existing dark theme.

Long replies have a maximum 200 px preview body, followed by a 48 px action
footer. A 36 px bottom fade indicates that more text follows. The footer contains
one clear **Expand response** action and a secondary Copy control. Remove the
character count, separate Read response button and all Check progress buttons
from the transcript. Task activity belongs in the existing Agents → Work area.

Short replies use their natural height and omit Expand and the fade. Copy can
remain secondary. Do not reserve the full preview height for a one-line answer.

## Reading behavior to implement after design acceptance

- Expand reveals the same saved reply inline. It replaces the footer label with
  **Collapse response** and adds a second Collapse control beside the author row.
- Expanded messages grow in normal conversation flow. The conversation supplies
  scrolling; the response box has no second scrollbar. Keep the composer in flow.
- Collapse returns to the same message anchor. Preserve the conversation draft,
  input focus and nearby reading position.
- Cap the preview during streaming too. Only the user's action expands it.
  Keep an explicitly expanded reply expanded as its saved content updates, while
  respecting existing saved-version and reading-position rules.
- Use an exact excerpt, not an invented summary. A phone wraps the same text.
  Retrieve the full saved body with existing source/access checks; expansion and
  Copy must not invoke an agent. Preserve loading, failed-read and revoked states.
- Give footer actions at least 44 px touch targets, keyboard activation and visible
  focus. Expand should expose its expanded state to assistive technology.
- Apply the limit to ordinary response prose. Approval, question, attachment and
  other explicit action controls retain their own visible interaction semantics.

## Canvas and verification

Live document at creation: `tab-2`, page `836672655:4`. Rediscover document/page IDs
before later editing; they can change across app restarts.

- Compact root: `836672655:752`, at canvas origin.
- Expanded root: `836672655:467`, to the right.
- Phone/behavior root: `836672655:867`, below the compact root.

All three PNGs were exported from OpenPencil and visually inspected. Local Adwaita
Sans faces were reported faithfully available, with no substitutions. The native
document was saved successfully. The boards are editable static concepts; the
Expand controls are visual states, not a wired interactive prototype. No application
tests were needed or run for this design-only change.
