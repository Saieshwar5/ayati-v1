# Cuckoo mobile app: decisions

The owner's decisions on the mobile app. Newer entries override anything in
`mobile-plan.md` and `mobile-design-ideas.md` in this folder; the repository copy
(`designs/mobile-app/`, branch `claude/gifted-albattani-tzbldf`) already includes
them.

## 2026-11-14

- **D251:**
  - Agents' native channels are Cuckoo Chat and email; WhatsApp and Telegram are
    removed (schema 113).
  - The local email and WhatsApp simulations stay as gateway test fixtures.
- **D252:**
  - Cuckoo gets a native React Native (Expo) app for members' daily work: Home,
    Chat, approvals, questions and notifications.
  - The web app stays the full product.
  - Phones sign in through Cuckoo with device sessions.
  - Push notifications never carry message content.
  - The app opens on Home, with five tabs: Home, Chats, Needs me, Agents, You.
- **D253, design choices:**
  1. Home panels are **read-only** on the phone in the first release. Adding,
     removing and reordering stay on the web; long-press reordering is later work.
  2. The phone **home-screen widget comes later**, after the first store release.
  3. Agents use their **real faces**: the bundled robots
     (`web/public/agent-avatars/robot-01.svg` to `robot-24.svg`) and uploaded
     photos, as on the web. The robot glyph in the design screens is a placeholder.
  4. **Tablets get two panes**, like the web: the chat list beside the open
     conversation, and the agents list beside the profile. Phones keep one pane
     with Back. The tablet screens still need drawing before M2 ends.
  5. **Muted text is `#5F6670`** in light mode on the web and the phone.
     - Applied in `web/src/workspace.css` and noted in `runtime/design-system.md`.
     - The old `#697079` was only 4.38:1 on the selection grey; the new value is
       5:1 or more on every light surface.

## Still open

- Swipe actions on chat rows (pin, mute, mark read).
- The tablet two-pane screens are not drawn yet.
- Owner set-up for M0:
  - Apple and Google developer accounts and an Expo account;
  - app name, icon and bundle id;
  - public or private store listing;
  - the mail domain for email.
