# Cuckoo phone app

Cuckoo's native app for iPhone and Android: Expo SDK 58 (React Native 0.88, New
Architecture), TypeScript and Expo Router. Plan and decisions:
[designs/mobile-app/](../designs/mobile-app/mobile-plan.md). Sign-in contract:
[runtime/mobile-sign-in.md](../runtime/mobile-sign-in.md).

Status (2026-11-14, M2):
- **Built:**
  - sign-in and workspace choice;
  - the five tabs, with icons and a Needs me badge;
  - Chats, and conversations with live replies, sending and Stop;
  - Home: business clock, Needs you and read-only saved panels in every family,
    each with its own page;
  - Needs me, Agents with each agent's profile, and You;
  - real agent faces (the bundled robots and uploaded pictures);
  - on tablets, Chats beside the open conversation and Agents beside the profile;
  - exact approvals and agent questions as sheets, from Needs me, Home and the
    reply that asked (M3);
  - photos (camera or library), files and voice notes, sent through the existing
    upload (10 MiB, eight per message) and shown in messages: photo thumbnails,
    voice notes with play and transcript, and files that download to the share
    sheet (M3).
  - push notifications: a one-time prompt on Home, a switch in You, and taps that
    open the exact conversation, approval or question (M4,
    [runtime/mobile-push.md](../runtime/mobile-push.md)).
- **Not yet:**
  - the Expo project id and store identities (M0), which push needs;
  - device testing.

## Layout

| Path | Holds |
| --- | --- |
| `src/app/` | Routes only (Expo Router). `_layout.tsx` guards: signed out → `sign-in`, no workspace → `workspace`, otherwise `(tabs)` and `chat/[id]` |
| `src/session/` | Sign-in in the system browser, the device-session token in SecureStore, the session context |
| `src/live/` | The live message stream (streaming fetch, cursor resume, reconnect) and its context |
| `src/chats/`, `src/needs-me/`, `src/agents/`, `src/you/`, `src/home/` | Each tab's hooks and components |
| `src/faces/` | Faces; `robots.ts` is generated from `web/public/agent-avatars` by `scripts/sync-agent-avatars.mjs` |
| `src/ui/`, `src/theme/` | Shared controls, tab icons, request hooks and the web's tokens |
| `../shared/` | `@cuckoo/shared`: API types, the request client and pure rules, with their own tests |

## Run it

1. Start the controller (root README) with the app's scheme allowed for
   development sign-in, in `.env`:

   ```sh
   AYATI_MOBILE_APP_SCHEME=cuckoo
   ```

2. Install and start the app:

   ```bash
   npm ci --prefix shared
   ```

   ```bash
   npm ci --prefix mobile
   ```

   ```bash
   cd mobile && npx expo run:ios
   ```

   The app uses native modules beyond Expo Go's set only through Expo's own
   modules, but a development build (`npx expo run:ios|android`, or an EAS
   development build) is the supported way to run it.

3. Point the app at the controller with `EXPO_PUBLIC_CUCKOO_ORIGIN` (default
   `http://127.0.0.1:8765`). The controller checks the `Host` header against its
   own origin, so the phone must reach it by that exact address:
   - **iOS Simulator:** the default works; the simulator shares the Mac's network.
   - **Android emulator or a USB phone:** `adb reverse tcp:8765 tcp:8765`, then the
     default works.
   - **A physical iPhone, or any phone over Wi-Fi:** run the controller behind
     HTTPS with `AYATI_PUBLIC_ORIGIN` and set `EXPO_PUBLIC_CUCKOO_ORIGIN` to it.
     Register that origin's `/auth/callback` in WorkOS.

Production builds use the universal link instead of the scheme:
`EXPO_PUBLIC_CUCKOO_UNIVERSAL_LINKS=1`, the server's `AYATI_MOBILE_IOS_APP_IDS` /
`AYATI_MOBILE_ANDROID_*`, and the Associated Domains entitlement. Those need the
store identities from M0 (app name, bundle id, team).

## Checks

```bash
npm run check --prefix mobile
```

```bash
npm test --prefix mobile
```

```bash
npm test --prefix shared
```

```bash
cd mobile && npx expo export --platform ios --platform android --output-dir /tmp/cuckoo-export
```

The export bundles both platforms with Metro, which catches resolution problems
(such as `@cuckoo/shared`) without a simulator. It is not a device test.

## Rules

- Requests go through `@cuckoo/shared`'s client: `Authorization: Bearer`, the
  `X-Ayati-Request` marker and `credentials: "omit"`. A request that carries a
  cookie is treated as a browser's and needs an Origin the app can't send.
- Screens are keyed by person and workspace (`src/app/_layout.tsx`), so a switch
  drops their data, drafts and the live stream.
- Sending keeps one message id per payload, so a retry can't post twice.
- Colours come from `src/theme/tokens.ts`, the web's values. Change both together.
- Add Expo modules with `npx expo install` so versions match the SDK.
