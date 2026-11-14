# Cuckoo mobile: store release checklist (M0 and M5)

Status: prepared 2026-11-14. Nothing here is submitted. Items marked **Owner** need
the owner's accounts or decisions. Build profiles are in `mobile/eas.json`.

## M0: accounts and identities (Owner)

- [ ] Apple Developer Program ($99 a year), Google Play developer account ($25 once)
      and an Expo account.
- [ ] App name, icon and bundle id (for example `app.cuckoo.mobile`). Then add
      `ios.bundleIdentifier`, `android.package` and the EAS project id to
      `mobile/app.json`.
  - The EAS project id (`npx eas-cli@latest init`) is also what push notifications
    need (`Constants.easConfig.projectId`).
  - Until it is set, the app says notifications are unavailable.
- [ ] The production origin. Set `EXPO_PUBLIC_CUCKOO_ORIGIN` in the EAS
      environment for `preview` and `production`; `eas.json` sets only the
      development value.
- [ ] Universal links on that origin, on the controller:
  - `AYATI_MOBILE_IOS_APP_IDS=TEAMID.bundle.id`;
  - `AYATI_MOBILE_ANDROID_PACKAGE`;
  - `AYATI_MOBILE_ANDROID_CERT_SHA256` (Play App Signing's certificate);
  - the Associated Domains entitlement (`applinks:` and `webcredentials:` for the
    domain) in `app.json`.
- [ ] Push: `AYATI_MOBILE_PUSH=expo` on the controller, an Expo access token file if
      the project uses enhanced push security, and APNs and FCM credentials in Expo.
      The controller must reach `exp.host`.
- [ ] Public listing, or unlisted or private distribution.
- [ ] WorkOS session settings checked against the 30-day device session: Cuckoo
      never refreshes WorkOS tokens, so a shorter WorkOS maximum length or
      inactivity timeout signs phones out sooner.

## Blocking decision: account deletion (Owner)

Apple: apps that support account creation must let people start deleting their
account inside the app ([App Review 5.1.1(v)](https://developer.apple.com/help/app-review/guideline-reference/5-1-1-account-deletion)).
A link to a website alone has been rejected. Google Play has a similar requirement
for apps that create accounts; confirm its current wording before submitting.

Cuckoo has no account deletion yet (open item O2). The app never offers sign-up,
and workspaces are joined only by invitation. However, the first sign-in creates
a Cuckoo account row for any verified WorkOS identity, even one with no workspace,
so review may still treat this as account creation. Choose one:

1. Build account deletion (server and in-app, under You); the safer path.
2. Prevent accounts from being created through the app: refuse the mobile sign-in
   for people who have no workspace membership or pending invitation, and say why
   in the review notes.

## M5: testing and submission

- [ ] Development build on real phones (`eas build --profile development`):
  - sign-in through the auth session: the scheme in development, the universal
    link in preview;
  - the live stream across backgrounding and network changes;
  - camera, photos, files, voice notes and the share sheet;
  - push delivery and opening the right place from a tap, warm and cold.
- [ ] TestFlight and Play internal testing with a real workspace.
- [ ] Privacy answers (draft below), review notes and screenshots.

### Privacy labels and Data safety: draft answers

What the app sends, from its code (`mobile/src`, `shared/src`):

| Data | Collected | Why | Linked to the person | Tracking |
| --- | --- | --- | --- | --- |
| Name, email | Yes (from the work account) | App functionality, account | Yes | No |
| Messages, photos, audio, files the person shares | Yes | App functionality | Yes | No |
| Device name and platform | Yes | Signed-in devices list | Yes | No |
| Push tokens | Yes | Notifications | Yes | No |
| Location, contacts, health, browsing, ads identifiers | No | — | — | — |

- No third-party analytics or advertising SDKs are included.
- Data is encrypted in transit (HTTPS in production).
- Deletion is subject to the account-deletion decision above.

### Review notes: draft

- Cuckoo is a workspace app for teams who work with AI agents. Sign-in uses the
  company's account through the system browser; Cuckoo never sees passwords.
- Provide a demo workspace and account for review. New people join a workspace
  only by an invitation from its admin.
- AI agents act only within permissions set by the workspace's admins.
  - Actions marked "asks first" need a person's exact approval in the app, and
    a notification can never approve anything.
  - Notifications show who and what kind, never message content.
