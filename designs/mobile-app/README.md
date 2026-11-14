# Cuckoo mobile: design ideas and screens

Status: first design, 2026-11-14. Not built. Source screens: `screens/*.dc.html` (Drive: `proj-docs/mobile/designs/`).
Interactive canvas: <https://claude.ai/artifact/229wpFmrBqZmLytfEJW6tj>.

## The idea in one line

The same calm, graphite-on-white Cuckoo as the web app, rebuilt for one thumb:
the daily work of talking to agents, unblocking them and following their work,
with everything else opening the web app.

## Principles

1. **Grey outside, white inside.** As on the web, where the grey rail sits outside
   the white workspace: the tab bar sits on the grey canvas (`#F3F4F5`) and every
   screen's content is the white surface.
2. **One thumb.** Main actions live at the bottom: tab bar, composer, Approve,
   Send answer. Every target is at least 44 px.
3. **Sheets, not new pages.** Approvals, questions, attachments and filters slide up
   from the bottom and keep the reader's place in the chat.
4. **Agents look like teammates.** Round faces like people; an agent is told apart
   only by the small `AI` tag after its name (web rule).
5. **Colour only where it means something.** Red: needs you. Purple: an agent is
   working. Green: online or done. Amber: a problem or paused. Everything else is
   graphite, grey and white.
6. **Nothing private on the lock screen.** Notifications say who and what kind,
   never message text, files or amounts.
7. **Honest scope.** Anything not built for phones says so and opens the web app.

## Tokens (from `web/src/workspace.css`)

Use the web's semantic tokens; never invent a parallel palette.

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| surface | `#FFFFFF` | `#1C1F26` | Screen content |
| canvas | `#F3F4F5` | `#111318` | Tab bar, grey grounds |
| raised | `#FFFFFF` | `#252A34` | Sheets, composer (dark) |
| soft | `#F7F8F9` | `#252A34` | Agent reply box, search, clock chip |
| selection | `#EEF0F2` | `#303540` | Own messages, selected segments |
| text | `#22252A` | `#F0F1F5` | Body text, primary buttons (light) |
| muted | `#5F6670`* | `#ABB1BF` | Meta text, inactive tabs |
| border | `#E5E7EA` | `#303540` | Row dividers |
| border-panel | `#D1D6DD` | `#414956` | Home panels, cards, approval details |
| border-control | `#AEB8C5` | `#697487` | Composer, outlined buttons, inputs |
| working | `#6257CF` | `#AFA5FF` | Agent working |
| online / success | `#1F9D55` / `#26745C` | `#4CC38A` / `#85C9A7` | Online dot, done ticks |
| danger | `#A42F35` | `#FFADAF` | Needs-you counts, Sign out |
| warning | `#945B10` | `#EFBC77` | Problems, paused |

\* The web's muted is `#697079`; phones use the slightly darker `#5F6670` so 11–13 px
labels on the grey tab bar keep 4.5:1 contrast. Decide whether to change the web too.

Type: Inter. Large title 30–32 bold; header 17 semibold; message text 16 / 1.45;
meta 12–13. Corners: 12 px buttons and inputs, 14–16 px cards and reply boxes,
18 px composer and bubbles, 22 px sheet tops. Shadows only on the composer
(`0 4px 12px` ink at 8%) and sheets.

## Navigation: five tabs

| Tab | Holds | Notes |
| --- | --- | --- |
| **Home** | Business clock, what needs you, agents working now, saved panels | The app opens here |
| **Chats** | Pinned, direct messages (agents and people), channels | Filters: All, Unread, Agents, People, Channels |
| **Needs me** | Approvals, questions, problems; finished work | Red badge = things only you can unblock |
| **Agents** | Every agent you may use, with live status | Tap: profile, then Message |
| **You** | Profile, status, workspace switch, settings, sign out | |

Conversations, profiles, panels and sheets open on top of a tab and hide the tab
bar; Back returns to the same scroll position.

**Opens the web app instead:**
- creating and editing agents, tools and rules;
- Settings, People, company rules, Usage;
- Apps, built apps;
- arranging and creating Home panels;
- Library management;
- watching or taking over an agent's computer or browser.

## Why Home is a tab, and what it shows

The web opens on Home: a business clock, Notifications and the person's saved
panels. Without Home, the phone would lose the "how is my business today" view and
force people into chats to find it. Home on the phone is a read-and-react page,
not an editor.

Order from top to bottom (screen 5):
1. **Greeting and date, with the business clock** as a small soft chip (time and
   city), as on the web.
2. **Needs you**: the count and the top two items (Approve, Answer), each one tap
   away, with See all opening Needs me. It hides when nothing waits.
3. **Working now**: chips for agents currently working on your requests, with their
   step ("Nova · 2 of 4"). Tap opens that conversation. It hides when none are.
4. **Your panels**: the person's saved Home panels in one column, in their web order.
   - Each panel is a bordered card: title, the value in its family's shape, and
     "who keeps it · when updated". Families: metric, chart (line, bar, area,
     stacked bar, donut), table, collection, progress, timeline, pipeline, text.
   - Wide content shrinks to the phone: charts to a short strip, and tables to 2
     columns with "more".
   - "Arrange on the web": adding, removing and reordering stays on the web in the
     first release.

Opening a panel (screen 6) shows it full screen:
- the large value and full chart;
- "Show the numbers", an accessible values table;
- About this panel: which agent keeps it, how often, the source, and freshness and
  health ("Updated 10:36 AM · on schedule");
- Ask about this, which opens a chat with that agent;
- Refresh, which asks for an update only where the web allows it.

Rules carried over from the web: no invented business metrics; panels show only
what the person may see; viewing starts no agent work; Notifications counts are
pending items, not unread messages.

Later: reorder panels with a long press; "Today" summary written by an agent; a
widget for the phone's home screen showing one panel.

## Screens

Numbers match the canvas. Each is 390 × 844 (an iPhone-size frame) with the top
safe area left empty (no fake status bar).

### Getting in
1. **Sign in (`SignIn`)**: logo, one promise, "Sign in with your work account"
   opening the system browser, "Cuckoo never sees your password", "New here? Ask
   your admin for an invitation" (the app does not create accounts; see the
   account-deletion risk in the plan).
2. **Choose a workspace (`Workspace`)**: one card per workspace with role and size;
   pending invitations shown but accepted on the web or by email; Sign out.
3. **Turn on notifications (`Notifications`)**: why (agent replied, approval or
   question waiting, mentioned), the privacy promise, Turn on / Not now. Asked once,
   after sign-in, never at first launch.
4. **Lock screen (`LockScreen`)**: what notifications look like. Title: "Nova replied
   to you". Body: workspace and a generic phrase. Tap opens the exact place.

### Home
5. **Home (`Home`)**, see above.
6. **A panel, opened (`HomePanel`)**, see above.

### Chat
7. **Chats (`Chats`)**:
   - Large title, workspace name above it, New message, search and filter chips.
   - Rows are 68 px: face, name with AI tag, preview, time, count.
   - Counts are red when something is for you (an @mention, or an agent's reply to
     your request) and grey when muted. Previews show live state, such as "Waiting
     for your approval" in red and "Working · step 3 of 5" in purple.
   - Online dot on people. Swipe a row to pin or mute (to design).
8. **Talking to an agent (`AgentChat`)**:
   - Slim header (back, face, name, job; tap opens the profile).
   - Own messages on the right in a selection-fill bubble with time and "Seen".
     Agent replies sit under the author row in the soft box.
   - Live task progress inside the reply: steps tick off, with the current step in
     purple and **Stop**.
   - Floating composer: + (attach), text, mic.
9. **Approve an action (`Approval`)**:
   - A sheet over the dimmed chat with the agent, expiry and an exact details table
     (action, tool, ids, recipient, amount, due).
   - "Approving allows this one action exactly as shown", with Decline / Approve.
   - Never approvable from a notification or a chat message.
10. **A channel (`Channel`)**:
    - Header with name, members and agents, a notifications bell and details.
    - Others on the left with 32 px faces; you on the right.
    - @mentions highlighted, agent replies in the soft box, thread "2 replies", and
      a "1 new" line where you stopped reading.
    - Composer hint: "@ to ask an agent".
11. **Voice note and attachments (`Composer`)**:
    - Recording replaces the composer row: red dot, timer, live level, cancel ×,
      send. It stops at 5 minutes, as on the web.
    - A sent voice note shows play, waveform, length and a folded Transcript.
    - Photos show as a thumbnail with upload progress.
    - The attach row offers Camera, Photos, File and Library.

### Needs me
12. **Needs me (`Inbox`)**:
    - Segmented control: Waiting · 3 / Done recently.
    - Cards labelled by kind (Approval red, Question purple, Problem amber), with
      agent, summary, time or expiry and one action (Review, Answer, Fix in the web
      app).
    - Then "Finished for you today".
13. **Answer a question (`Question`)**:
    - A full-height sheet: agent, the question, large radio choices (with the
      agent's suggestion tagged) and an optional note.
    - Send answer, with the note "Answering is not an approval."

### Agents and you
14. **Agents (`Agents`)**:
    - Search, then flat rows: face, name and AI tag, job and what it handles, and
      live status (Working, Waiting for your approval, Ready, Paused by an admin).
    - Footer: create or change agents on the web.
15. **Agent profile (`AgentProfile`)**:
    - Big face, name, job and manager; Message and "Its work for you".
    - What it can do, marked ✓ or "asks you first"; what it knows (Library
      sources); a link to change it on the web.
16. **You (`You`)**:
    - Face with online dot, name, job and role; Set a status.
    - Workspace (switch); Notifications, Appearance, Signed-in devices, Edit profile.
    - Open Cuckoo on the web; Sign out.
17. **Dark mode (`AgentChatDark`)**:
    - The conversation in dark tokens.
    - A long agent reply folds to a fixed height with a fade and a corner-arrow
      expand button (the web's design).
    - An agent-made file shows as a download card.

## Interaction rules

- **Back** keeps scroll position and drafts. Drafts are saved per person and
  workspace, and cleared on sign out.
- **Pull to refresh** on lists. Live updates arrive by the message stream; never
  rewrite the whole transcript for each new token.
- **Haptics:** a light tap on send and approve; a warning buzz on a failed send.
- **Errors stay where they happened:** "Not sent · Retry" under a message; a failed
  approval keeps the sheet open with the reason.
- **Double taps never act twice:** buttons disable while saving.
- **Accessibility:**
  - Dynamic Type grows text, and rows grow with it.
  - Every icon button has a name ("Back to Chats", "Record a voice note").
  - Colour is always paired with words ("Waiting for your approval", not only red).
  - Reduced motion turns off spinners and sheet bounce.
- **Offline:** a thin banner under the header with Retry; sending waits and says so.

## Open design questions

1. Swipe actions on chat rows (pin, mute, mark read): which, and are they discoverable?
2. Should Home allow reordering panels on the phone in the first release?
3. A home-screen widget (one panel or "3 things need you"), now or later?
4. Real agent faces: reuse `web/public/agent-avatars/robot-*.svg` and uploaded photos
   instead of the placeholder robot glyph.
5. Tablet layout: a two-pane Chat (list and conversation), as on the web, or the
   phone layout scaled up?
6. Should muted on phones be `#5F6670` everywhere, including the web?

## Changing these designs

Edit `screens/<Screen>.dc.html` (see `screens/README.md`). Keep tokens and rules
above; update this file when a decision changes, with the date and why.
