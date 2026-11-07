# Chat and account improvements (proposal)

Waiting for the owner's approval; nothing is built. Editable source:
[chat-and-account.fig](chat-and-account.fig) (OpenPencil). The screens build on the
[chat collaboration](../chat-collaboration/README.md) and [chat fixes](../chat-fixes/README.md)
designs: the same faces, lists, robot avatars and floating composer. The rail is the
labelled main menu from the approved [Settings](../settings-simplified/README.md) and
[Agents](../agents-simplified/README.md) designs.

| File | Screen and notes |
| --- | --- |
| `1-unread-badge.png` | Home with a red count on Chat in the rail. (1) DMs plus @mentions and replies to you, on every page. (2) Home Notifications stays agent attention. (3) A dark dot when only other channel messages are unread; muted adds nothing. (4) Tab title "(3) Cuckoo". (5) Reading anywhere clears the count. |
| `2-presence.png` | Direct message with Meera. (1) Green dot = Cuckoo open in the last minute; no dot = away; agents have none. (2) The header shows "Online · In a meeting · until 11:30 AM" or "Last seen" instead of "Connected". (3) Status emoji after names in the list. (4) Profile shows the status. (5) Your connection shows only as an offline/reconnecting banner. (6) The rail count matches the list. |
| `3-status.png` | Set a status. (1) Available, Working, Busy (quieter alerts), In a meeting, Away. (2) Your own text and emoji. (3) Clear after 1 hour, today, this week or never. (4) Opened from the account menu; shown on profiles, in the list and in the @mention list. (5) "Show when I'm online" in Settings → Preferences. |
| `4-voice-messages.png` | Recording in the composer. (1) Tap the mic: red dot, timer, live level. (2) Send or Enter sends at once. (3) The same mic button, now a mic with ×, cancels; Escape too. (4) Player with play/pause, waveform and length. (5) Transcript under each voice message; agents receive the transcript. (6) Own voice messages keep ticks; failed uploads offer Retry. |
| `5-channel-details.png` | Details as a panel in the workspace. (1) Name once; quiet actions: notifications, Pin, Archive. (2) Archive near the top. (3) About. (4) One list of people and agents with search, Add people, Add agent. (5) Row menu with Remove. (6) Avatar stack in the header. |
| `6-details-states.png` | (1–3) Archive review in place, with unfinished agent work and one red button. (4–5) Channel settings for the creator and admins, with one Save. (6) Leave channel. (7–8) The direct-message variant: profile, presence, status, notifications; no settings or archive. |
| `7-add-skills.png` | Add skills during New agent, step 2. (1) One panel over the create flow. (2) Search. (3) Short groups. (4) Flat rows; Add attaches at once. (5) Unusable skills greyed with the reason. (6) Upload a skill opens a second step. |
| `8-upload-skill.png` | Upload a skill, in the same panel. (1) Back arrow. (2) Drop zone, then the checked file. (3) What Cuckoo read from SKILL.md. (4) One file list with an inline preview. (5) Plain warnings. (6) Share with the workspace (admins). (7) Cancel / Add skill. |
| `9-account-menu.png` | The account menu over Settings → Profile. (1) Only you, workspaces and Sign out. (2) Status line. (3) Workspace switcher. (4) Settings; Edit profile, My account and Appearance removed. (5) Sign out. (6) Profile is the one place to edit name, photo, title and About. |
| `10-phone.png` | Phone: (1) presence in the header, (2) the recorder, (3) voice messages, (4) dots, status and counts in the list. |
| `11-dark.png` | The voice message screen in the dark theme. |

## Behaviour changes to confirm

- **Voice:** Send now sends a recording straight away. The design system says Stop
  attaches audio to the draft and Send stays explicit; that rule would change.
- **Status:** Busy makes notifications quieter (only DMs and @mentions alert).
- **Header:** the connection label leaves the conversation header; the design
  system says to keep it there.
- **Details:** "Who can post" and changing public/private are new settings and need
  server support. Archive keeps today's rules: creator or admin, accepted work may finish.
- **Account menu:** Edit profile, My account and Appearance are removed. Their
  settings stay in Settings → Profile and Preferences.

Content uses a fictional company.
