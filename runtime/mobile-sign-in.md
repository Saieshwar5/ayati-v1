# Phone app sign-in and device sessions

Status: server side built and tested 2026-11-14 (mobile plan M1, schema 115). The
phone app itself is not built yet. Plan: [designs/mobile-app/mobile-plan.md](../designs/mobile-app/mobile-plan.md).

The web keeps its HttpOnly session cookie. A phone app can't rely on browser
cookies, so it gets a **device session**: a random bearer token. Cuckoo remains the
only party that talks to WorkOS, and the WorkOS secret never reaches the phone.

## The flow

1. The app makes a PKCE verifier (43–128 characters, RFC 7636) and its S256
   challenge, `BASE64URL(SHA-256(verifier))` without padding (43 characters).
2. It opens `GET /auth/mobile/start?challenge=<challenge>` in the system browser
   (`expo-web-browser`'s auth session). The optional `method=google` and `email=`
   work as on `/auth/login`. Invitations are not accepted here; they are
   accepted on the web or by email.
3. Cuckoo runs its normal WorkOS sign-in: the same single-use flow cookie, state
   and server-side PKCE, and the same `/auth/callback` registered in WorkOS. The
   flow remembers the app's challenge (`relay_login_flows.mobile_challenge`).
4. The callback sets **no** browser session. It stores a one-time code (its
   digest, the person, the WorkOS session and the challenge; 60 seconds) and
   redirects to:
   - `<origin>/auth/mobile/return?code=…`, the universal link (App Link on
     Android); or
   - `<scheme>://auth/return?code=…` when `AYATI_MOBILE_APP_SCHEME` is set, for
     development builds that have no verified universal link (local HTTP).

   A failed sign-in returns to the same address with `error=sign_in_failed`.
5. The app posts `POST /api/identity/mobile-sessions` with
   `{"code","verifier","device_name","platform"}` (`ios` or `android`; names 1–80
   characters). The code is deleted on the first attempt, right or wrong. A wrong,
   expired or reused code gets one message: "Sign-in code is invalid or expired".
6. The answer is `{"token","id","expires"}`. The app keeps the token in
   `expo-secure-store` and sends `Authorization: Bearer <token>` with
   `x-ayati-request: identity` and **no cookies** (`credentials: "omit"`).
   Cuckoo stores only the token's SHA-256 digest.

An intercepted code is useless without the verifier, which never leaves the app.

## Requests from a phone

- `identity()` reads the cookie first, then the bearer token, and checks the
  device session's WorkOS session exactly as for a browser: live for changes,
  trusting a positive answer for up to 60 seconds for reads
  ([provider/recent.rs](../src/identity/provider/recent.rs)).
- Membership and every workspace rule are read from PostgreSQL on each request,
  so leaving or being removed from a workspace cuts the phone off at once.
- **Origin rule:** browsers must send the canonical `Origin` on every change. A
  request with a bearer token and no `Cookie` header, or the code exchange
  without a cookie, skips only that check: with no cookie there is no ambient
  credential to forge. `x-ayati-request: identity` is still required. A request
  that carries both a cookie and a bearer token is treated as a browser's.
- A device session lasts **30 days** (`DEVICE_SECONDS`), or less if its WorkOS
  session ends first. `last_used` is updated at most every five minutes.

## Devices and sign-out

| Route | Does |
| --- | --- |
| `GET /api/identity/devices` | The person's signed-in phones: id, name, platform, created, last used, expiry, and `current` for the phone asking |
| `POST /api/identity/devices/{id}/revoke` | Signs that phone out and ends its WorkOS session (best effort) |
| `POST /api/identity/logout` with a bearer token | Deletes this phone's session and ends its WorkOS session |

Both work from the web (cookie) and from a phone. Each sign-in, revocation and
sign-out writes an audit entry (`device_signed_in`, `device_revoked`,
`device_signed_out`). From M4, signing out also removes the phone's push
registration.

## Universal-link files

Served only once the store identities exist (mobile plan M0):

- `/.well-known/apple-app-site-association` from `AYATI_MOBILE_IOS_APP_IDS`
  (`TEAMID.bundle.id`, comma separated): `applinks` for `/auth/mobile/return` and
  `webcredentials`, which iOS needs for an `https` auth-session callback.
- `/.well-known/assetlinks.json` from `AYATI_MOBILE_ANDROID_PACKAGE` and
  `AYATI_MOBILE_ANDROID_CERT_SHA256`.

Both answer 404 when unset. `/auth/mobile/return` itself shows a short "Return to
the Cuckoo app" page for when the app did not open.

## Tests

`src/identity/postgres/device_sessions/` runs the whole journey against real
PostgreSQL and a fake WorkOS: a wrong verifier burns the code, expired and reused
codes, a missing request marker, bearer reads and writes without Origin, a cookie
plus bearer refused, a forged token, WorkOS ending the session, device expiry,
removal from a workspace, listing, revoking another phone, sign-out, the
development scheme, a failed sign-in and both universal-link files.

## Still to check

- On real devices: `expo-web-browser`'s auth session with an `https` universal-link
  callback (iOS 17.4+ supports it), and Android Custom Tabs returning by App Link.
- WorkOS session settings: Cuckoo never refreshes WorkOS tokens. If AuthKit has an
  inactivity timeout or a maximum session length under 30 days, phones are signed
  out sooner. Confirm the WorkOS session settings before the first store release.
