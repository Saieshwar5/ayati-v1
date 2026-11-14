# Push notifications to phones

Status: built and tested 2026-11-14 (mobile plan M4, schema 116). Sending needs
the Expo project from M0, and real delivery has not been checked on a device.
Sign-in and device sessions: [mobile-sign-in.md](mobile-sign-in.md).

## What is pushed

The controller (`src/identity/push.rs`) asks PostgreSQL every 5 seconds what is due
(`src/identity/postgres/device_sessions/push_due.sql`). It looks only at people who
have a phone with notifications on, and finds:

- **Messages** that are still unread 10 seconds after they arrived, by the same
  rule Chat uses for unread ([unread.rs](../src/identity/postgres/unread.rs)):
  - from someone else, sent or finished, never an agent's progress note;
  - in an active conversation the person can see, in an open workspace;
  - arrived after they joined;
  - with no seen receipt on any device.

  The delay is what stops a double notification: a message read on the web in
  those 10 seconds is never pushed.
- **Levels**: the conversation's own level (`relay_chat_notify`). Without one,
  direct messages notify for everything, and channels follow the person's default
  (`channel_level`, normally mentions). `mentions` means an @mention of them, or
  an agent's reply to their request; `none` never notifies.
- **Approvals** waiting for them, unless they turned approval alerts off
  (`approval_alerts`).
- **Questions** an agent is asking them.

Only items from the last 10 minutes are considered. Each one is claimed in
`relay_push_sent` before it is sent, so it goes out at most once, even with
several controllers. If Expo is unreachable, the claimed items are not retried: a
missed push is preferred to a double one.

## What a notification says

| Item | Title | Body |
| --- | --- | --- |
| Agent's reply to the person's request | Nova replied to you | Workspace name |
| @mention | Asha mentioned you | Workspace name |
| Direct message | Asha sent you a message | Workspace name |
| Channel set to everything | Asha posted in a channel | Workspace name |
| Approval | Nova needs your approval | Workspace name |
| Question | Nova has a question for you | Workspace name |

Never message text, files or amounts. The data names only the workspace, the kind
and the item's id. The app opens that place and checks access again, switching
workspace first if the person is still in it. A notification never approves or
answers anything itself.

## Registrations

| Route (bearer token, no cookie) | Does |
| --- | --- |
| `POST /api/identity/devices/current/push` `{expo_token, native_token?}` | Turns notifications on for this phone |
| `POST /api/identity/devices/current/push/remove` | Turns them off |

- **Storage:** `relay_push_devices` belongs to the device session (`ON DELETE
  CASCADE`), so signing out, revoking the phone or the session expiring removes it.
- **One phone, one registration:** a token moves to the phone's newest session.
- **Rejected tokens:** a token Expo reports as `DeviceNotRegistered` is dropped.
- **Native tokens:** the APNs or FCM token is kept too, so sending can move straight
  to Apple and Google without an app change.
- **Devices list:** `GET /api/identity/devices` includes `notifications` for each
  device.

## Configuration

```sh
AYATI_MOBILE_PUSH=expo
AYATI_EXPO_ACCESS_TOKEN_FILE=/absolute/private/expo-access-token   # if the Expo project requires one
```

The controller must reach `exp.host`. Executors refuse the token file.

## The app

- **Asking:** the app asks once, after sign-in, with a card on Home ("Turn on
  notifications" / "Not now"). There is also a switch in You › Notifications on
  this phone.
- **Availability:** notifications need a real phone and the Expo project id (M0).
  Until then the switch says why it is unavailable.
- **While the app is open:** no banner is shown, because the live stream already
  shows the message.

## Tests

- **`push_tests.rs` (PostgreSQL):**
  - the reading delay, mentions versus plain channel messages, and direct
    messages;
  - a seen receipt and the `none` level stopping a push;
  - nobody without a phone being due, and claiming once;
  - a token moving to the phone's newest session, sign-out removing the
    registration, and invalid tokens refused;
  - the sender against a fake Expo: one message per phone, the right words, no
    message text, a dropped token, and nothing sent twice.
- **The app:** opening the exact target, switching workspace, and opening nothing
  for a workspace the person left or for malformed data.

## Still to do

- **Device checks:** the auth prompt, token registration, and delivery on iOS and
  Android.
- **Expo push receipts:** checking receipts after the ticket, for errors that
  arrive later.
- **Token rotation:** a new token is followed only while You is open. A token that
  is dropped meanwhile shows as off, and the person turns it on again.
- **"Long task finished":** covered as the agent's reply to the request. There is
  no separate rule.
