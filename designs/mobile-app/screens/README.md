# Cuckoo mobile app designs (raw source)

The first design of Cuckoo's phone app, 2026-11-14: one file per screen, kept raw
so people and agents can read and change it. Design rules and the reasons behind
each screen: `../README.md` here, or `proj-docs/mobile/mobile-design-ideas.md` in Drive
(repository). The interactive canvas is the Claude artifact
<https://claude.ai/artifact/229wpFmrBqZmLytfEJW6tj>.

## Files

| File | Screen |
| --- | --- |
| `canvas.json` | Canvas index: each screen's position, size and title, and the row titles |
| `Main.dc.html` | Design language sheet: principles, tabs, colours, type, building blocks |
| `SignIn.dc.html` | 1 Sign in |
| `Workspace.dc.html` | 2 Choose workspace |
| `Notifications.dc.html` | 3 Turn on notifications |
| `LockScreen.dc.html` | 4 A notification arrives |
| `Home.dc.html` | 5 Home |
| `HomePanel.dc.html` | 6 A panel, opened |
| `Chats.dc.html` | 7 Chats |
| `AgentChat.dc.html` | 8 Talking to an agent |
| `Approval.dc.html` | 9 Approve an action |
| `Channel.dc.html` | 10 A channel |
| `Composer.dc.html` | 11 Voice note and attachments |
| `Inbox.dc.html` | 12 Needs me |
| `Question.dc.html` | 13 Answer an agent's question |
| `Agents.dc.html` | 14 Agents |
| `AgentProfile.dc.html` | 15 Agent profile |
| `You.dc.html` | 16 You |
| `AgentChatDark.dc.html` | 17 Dark mode |

## Format

Each `.dc.html` is a self-contained "Design Component" page:
- Plain HTML and CSS, and the Inter font from Google Fonts.
- Markup inside `<x-dc>`, page CSS in `<helmet><style>`, and a small
  `<script type="text/x-dc" data-dc-script>` block.
- Phone screens are fixed at 390 × 844 with the top safe area left empty.
- Links between screens (`<a href="Chats.dc.html">`) make the canvas clickable.
- Icons are inline SVG; the Cuckoo mark is the real path from
  `web/public/brand/cuckoo-symbol.svg`. Agent faces are placeholder robot glyphs on
  tinted circles.

`./support.js` is the canvas runtime; it is not in this folder. To look at a file
in a plain browser, delete that `<script>` line and the `<helmet>`/`<x-dc>` tags
still render as ordinary HTML. To change the live canvas, publish the edited file
back to the artifact.

These are designs, not app code: the React Native app rebuilds them with native
components, using the same tokens and layout.
