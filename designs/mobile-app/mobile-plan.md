# Cuckoo mobile app: build plan

Status: M1 (server sign-in and device sessions) built and tested 2026-11-14, schema
115, see [runtime/mobile-sign-in.md](../../runtime/mobile-sign-in.md). M2 started:
`mobile/` and `shared/` hold sign-in, workspace choice, the five tabs, Chats, live
conversations with Stop, Home with read-only panels, real faces and two-pane Chats
and Agents on tablets, and agent profiles ([mobile/README.md](../../mobile/README.md)).
Device checks remain before M2 is closed. Supersedes the Google Doc "Cuckoo Mobile
App Plan" of the same day by adding Home. Design: `mobile-design-ideas.md`.

## Summary

Cuckoo has three surfaces:
- **Public website:** later, separate from the product.
- **Web app:** the full product on desktop; it keeps working on phones.
- **Native app:** iPhone and Android, built with React Native and Expo, for members'
  daily work with agents.

Agents' native channels are Cuckoo Chat and email; WhatsApp and Telegram were
removed (schema 113, branch `claude/gifted-albattani-tzbldf`).

Build the server side first: mobile sign-in, device sessions and push. Then a
narrow app whose native screens are Home, Chats, Needs me, Agents and You; agent
setup, Settings, Apps and Library management open the web app. Expect about 2 to
3 months to a first store release.

## Why React Native, and what it costs

- **Gains:**
  - native scrolling, keyboard and gestures in long chats;
  - reliable push notifications;
  - camera, voice notes and files;
  - listings in both app stores.

  It is the same language (TypeScript) and the same React knowledge as the web
  client.
- **Cost:** the web client (232 components, about 22,000 lines of TypeScript and
  3,500 of CSS) draws HTML and CSS, which React Native does not use. Every mobile
  screen is new, and every phone feature is built twice from now on.
- **Rule:** keep the app narrow, share everything that is not a screen, and open
  the web app for the rest.

## Technology

- **Framework:** Expo SDK 58 (React Native 0.88, New Architecture), TypeScript and
  Expo Router. EAS does builds, store submission and over-the-air JavaScript
  updates. Confirm the newest stable SDK when starting.
- **Sign-in:**
  - `expo-web-browser` opens the sign-in browser session;
  - `expo-secure-store` keeps the session token (Keychain on iPhone, Keystore on
    Android).
- **Notifications:** `expo-notifications` through Expo's push service. Store both
  the Expo token and the device's native token, so sending can move straight to
  APNs and FCM without an app change.
- **Media:** `expo-audio` for voice notes; `expo-image-picker` and
  `expo-document-picker` for photos and files.
- **Charts for Home panels:** `react-native-svg`, drawn with the same rules as the
  web panel families.
- **Live updates:** the existing message-events stream (Server-Sent Events), read
  with streaming fetch, resuming from its cursor.
- **Tests:**
  - screens: Jest and React Native Testing Library;
  - device journeys: Maestro;
  - server: PostgreSQL tests.

## Repository layout

- `web/` stays as it is; `mobile/` holds the Expo app.
- `shared/` is a TypeScript package used by both. It holds:
  - API types;
  - request functions that take their `fetch` and session as inputs;
  - pure rules: message, approval and panel models, and markdown parsing.

  Code moves into it only when the mobile app needs it.
- One npm workspace, with lockfiles tracked.
- `designs/mobile-app/` holds the design source.

## Sign-in on phones (server)

The web keeps its HttpOnly session cookie. A phone app can't rely on browser
cookies, so it gets a **device session**. Cuckoo stays the only place that talks to
WorkOS.

1. The app makes a PKCE verifier and challenge, then opens
   `{origin}/auth/mobile/start?challenge=…` in the system browser.
2. Cuckoo runs its normal WorkOS sign-in, then sends the browser back to the app
   with a one-time code that lasts 60 seconds. It uses a universal link (an App
   Link on Android).
3. The app posts the code and verifier to `POST /api/identity/mobile-sessions`.
   It gets back a random device-session token. Cuckoo keeps only its digest, with
   device name, last use and expiry.
4. The app keeps the token in secure storage and sends `Authorization: Bearer`.
   - Cuckoo checks the WorkOS session exactly as it does for cookies.
   - The browser Origin check is skipped only for requests that carry a bearer
     token and no cookie.
5. Revoking and leaving:
   - Profile › Devices lists the sessions and revokes them.
   - Signing out deletes the session and its push registration.
   - Workspace access is checked on every request, so leaving a workspace cuts the
     phone off too.

## Push notifications (server)

- **Device registry:** a table of each device session's Expo token, native token and
  platform. A row is removed on sign-out or revocation, and when Apple or Google
  report the token unregistered.
- **Events:**
  - an agent replied to you;
  - an approval or question waits for you;
  - someone mentioned you;
  - a long task finished.

  Each person's existing notification settings apply.
- **Content:** the name and a short phrase ("Replied to you", "Needs your
  approval"), never message text, files or amounts.
- **Behaviour:**
  - Tapping a notification opens the exact conversation, approval or question.
  - No notification while the person is reading that conversation on the web.
- **Secret:** the Expo access token sits in an operator file on the controller.

## First release: what is native

| Native in the app | Opens the web app |
| --- | --- |
| Sign in, choose a workspace, sign out, devices | Creating and editing agents, tools and rules |
| Home: business clock, Needs you summary, agents working now, saved panels (read-only), panel detail | Arranging and creating Home panels |
| Chats: DMs with agents and people, channels, threads | Settings, People, company rules, Usage |
| Conversation: messages, live replies and task progress, Stop | Apps and built apps |
| Photos and files (send, view, download); voice notes with transcript | Library management |
| Approvals and questions, inline and in Needs me | Watching or taking over an agent's computer or browser |
| Agents list and profile; start a conversation | Activity beyond Needs me |
| Push notifications and their settings; appearance; status | |

Recent messages are kept in memory only in the first release. Drafts are kept per
person and workspace and cleared on sign out.

## Milestones

- **M0, accounts and decisions (owner):**
  - an Apple Developer Program account ($99 a year), a Google Play developer
    account ($25 once) and an Expo account;
  - app name, icon and bundle id (for example `app.cuckoo.mobile`);
  - the domain for universal links;
  - public listing, or unlisted or private distribution.
- **M1, mobile sign-in and device sessions (server), done 2026-11-14:**
  - the start and exchange routes, the device-session table and bearer identity;
  - the Origin rule for bearer requests;
  - devices list and revoke;
  - universal-link files;
  - PostgreSQL tests for a wrong verifier, an expired or reused code, revocation, a
    removed member, and bearer without cookie.
- **M2, app skeleton, Chat and Home (in progress; first increment 2026-11-14):**
  - the Expo app and the `shared/` package;
  - sign-in and the workspace picker;
  - the chat list, and a conversation with live replies and sending;
  - Home with clock, Needs you summary and read-only panels in all families, plus
    panel detail;
  - light and dark themes from the tokens;
  - real agent faces: the bundled robots and uploaded photos;
  - a two-pane layout on tablets for Chats and Agents.
- **M3, approvals, questions, files and voice (built 2026-11-14; device checks pending):**
  - exact approvals, with the web's rules for single submission and expiry;
  - questions and forms;
  - photo, file and voice-note upload through the existing upload and
    transcription.
- **M4, push notifications end to end (built 2026-11-14, schema 116; needs M0 to send):**
  - the device registry and the sender in the controller;
  - privacy-safe content and deep links;
  - no double notifications, and notification settings.
- **M5, internal testing and store release (checklist and draft answers: [store-release.md](store-release.md)):**
  - TestFlight and Play internal testing with a real workspace;
  - privacy labels and the Data safety form;
  - review notes about AI agents and company sign-in;
  - submission.

## Risks and checks

- **Account deletion:** Apple requires in-app account deletion when an app lets
  people create accounts. Cuckoo has no account deletion yet (open item O2). Either
  build it, or keep the app to people invited into existing workspaces (the design
  assumes this). Confirm Apple's current rule before M5.
- **Two clients:** every phone-facing feature needs a mobile version. Each feature
  plan should say what changes on mobile.
- **Streaming:** confirm on devices that the stream survives backgrounding and
  network changes, and resumes from its cursor.
- **Expo push** has no SLA. Keep native tokens so sending can move straight to Apple
  and Google.
- **Universal links** need the Apple and Android verification files on Cuckoo's
  domain.
- **Home panels on small screens:** check each family at 320–390 px; tables with
  three columns need a "more" view.

## Decisions to record

These are for `decisions.md`.

- **D251:** agents' native channels are Cuckoo Chat and email; WhatsApp and Telegram
  are removed (schema 114). The local email and WhatsApp simulations stay as gateway
  test fixtures.
- **D252:**
  - Cuckoo gets a native React Native (Expo) app for members' daily work: Home,
    Chat, approvals, questions and notifications. The web app stays the full
    product.
  - Phones sign in through Cuckoo with device sessions.
  - Push notifications never carry message content.
  - The app opens on Home, with five tabs: Home, Chats, Needs me, Agents, You.
- **D253 (design choices, 2026-11-14):**
  - Home panels are read-only on the phone in the first release.
  - The home-screen widget comes after the first store release.
  - Agents show their real faces: the bundled robots and uploaded photos.
  - Tablets get two panes, like the web.
  - The light-mode muted token is `#5F6670` on the web and the phone.
