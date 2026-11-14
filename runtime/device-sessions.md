# Device sessions: signing in on a phone with a QR code

Status: built 2026-11-14 (schema 117). The server side and the web side are done;
the Cuckoo mobile app (`designs/mobile-app/mobile-plan.md`, milestone M1) does not
exist yet and is the first client.

A phone signs in by scanning a one-time code shown on the web. It then holds a
**device session**: a random token it sends as `Authorization: Bearer <token>`.
The web keeps its HttpOnly session cookie and nothing about cookies changed.

## The journey

1. In the web app, **Settings › Profile › Your phone › Sign in on your phone** makes
   a one-time code and shows it as a QR code with a countdown ("Works once, for 10
   minutes … Don't share it: whoever scans it signs in as you."), **New code** and
   **Hide**.
2. The Cuckoo app scans it and redeems the code. The web page notices the new phone
   (it rechecks the list every 4 seconds while the code waits), hides the code and
   says "Signed in on Ravi's iPhone."
3. **Signed-in devices**, below, lists every signed-in phone with its name,
   platform, sign-in date and last use, and a **Sign out** button for each.

Code: `src/identity/devices.rs` (routes and bearer identity),
`src/identity/postgres/device_sessions.rs` and `device_sessions.sql` (storage),
`web/src/react/settings/phone-sign-in.tsx`.

## API

All paths are on the Cuckoo origin. Times are Unix seconds. Errors are
`{"error": "..."}`.

| Request | Who | Answer |
| --- | --- | --- |
| `POST /api/identity/devices/link-code` | Web cookie session only | `{code, expires, expires_in, link, qr}` |
| `POST /api/identity/devices/redeem` with `{code, device_name, platform}` | Anyone holding the code; no session | `{token, device}` |
| `GET /api/identity/devices` | Cookie or bearer | `{devices: [{id, name, platform, created, last_used, expires, current}]}` |
| `POST /api/identity/devices/{id}/revoke` | Cookie or bearer, own devices only | `{revoked: true}`; 404 for anything else |

- **link-code:** a bearer request gets 403: a signed-in phone must not sign in
  more phones. `qr` is an SVG data URL of `link`; `expires_in` is 600. More than
  10 codes in an hour gets 429.
- **redeem:** `device_name` is trimmed, 1 to 60 characters, without control
  characters (for example "Ravi's iPhone"); `platform` is `ios` or `android`.
  Unknown, used and expired codes all get the same 400 answer. A person can have
  20 signed-in devices at once; past that, redeem is refused and the code stays
  usable once a device is signed out. `token` is shown only here.
- **devices:** never includes tokens. `current` is true for the phone asking; the
  web browser is not a device and is never listed.
- **revoke:** a phone may sign itself out with its own `device.id`. That is how
  the app signs out; `POST /api/identity/logout` only ends a cookie session.

With its token, a phone uses the same APIs as the web, for example
`GET /api/identity/session` (the person and their workspaces) and every
`/api/identity/workspaces/{id}/...` route.

## QR payload

```
cuckoo://sign-in?server=<origin, URL-encoded>&code=<code>
```

For example `cuckoo://sign-in?server=https%3A%2F%2Fcuckoo.example.com&code=Qm9…`.
`code` is 43 base64url characters (256 random bits). The QR code uses error
correction level M.

## Security decisions

- **Codes:** random 256-bit, stored only as a SHA-256 hash, valid 10 minutes,
  single use, tied to the person who made them. Making a new code replaces that
  person's previous one, used or not, so at most one code per person works.
  Guessing is not a practical attack at this size, so redeem has no attempt limit.
  **Hide** only hides the code on screen; it keeps working until it expires or a
  new code replaces it.
- **Exactly once:** redeem marks the code used with a single conditional `UPDATE`
  in the transaction that creates the device session. Of two concurrent redeems,
  the second waits for the first's row lock and then finds the code used.
- **Tokens:** random 256-bit, stored only as a SHA-256 hash, never logged. The
  audit log records `device_code_created`, `device_signed_in` and
  `device_signed_out` (the device ID, never a token).
- **Lifetime: 90 days, absolute.** A phone is personal and opened many times a
  day; asking for a fresh scan every few days would push people to keep codes
  around. 90 days bounds what a lost, unreported phone can do, matching common
  mobile-app sessions, and the person can sign it out at any time from the web.
  There is no sliding renewal; after 90 days the phone scans a new code. Signed-out
  and expired rows are deleted 30 days later, when the person next signs a phone in.
- **WorkOS:** a device session has no WorkOS session of its own (the web session
  that made the code lasts one hour and ends with web sign-out, so the phone
  cannot depend on it). Instead, every request checks that the WorkOS **account**
  still exists with the same verified email (`Provider::account_valid`), with the
  same freshness rule as cookies: changes ask WorkOS live; reads accept a positive
  answer up to 60 seconds old (`provider/recent.rs`). A deleted WorkOS account
  gets 401 on the phone. Signing out of the web does not sign phones out.
- **Cuckoo checks:** the device row (unrevoked, unexpired) and the person's account
  are read from PostgreSQL on every request, and every route's own authorization
  (workspace membership, roles, conversation access) applies unchanged because the
  bearer goes through the same `identity()` as cookies. Removing someone from a
  workspace cuts their phone off from it at once. Cuckoo has no account
  deactivation of its own; to cut a person off entirely, delete them in WorkOS.
- **Origin check:** browser requests that change something must carry Cuckoo's
  `Origin` and `X-Ayati-Request: identity` (a CSRF defence). A request with no
  session cookie authenticates only through what it sends explicitly, so the check
  is skipped for bearer requests without the cookie and for redeem without the
  cookie. A browser on another site cannot send an `Authorization` header without
  a CORS preflight, which Cuckoo never grants. With the cookie present, the cookie
  wins and the check applies as before.
- **Last used:** updated at most once a minute, so ordinary requests do not write.

## What the mobile app must do

1. Scan a QR code (or receive the `cuckoo://sign-in` link) and parse `server`
   and `code`. Accept `https` servers only (plain `http` only for local
   development), and show the server's host for the person to confirm before
   redeeming: a QR code from a stranger could point at another server.
2. `POST {server}/api/identity/devices/redeem` with JSON
   `{"code": "...", "device_name": "<the phone's name>", "platform": "ios" | "android"}`.
   No cookie, `Origin` or `X-Ayati-Request` is needed.
3. Keep `token` in secure storage (`expo-secure-store`: Keychain or Keystore) and
   `device.id` beside it. Never log the token or put it in a URL.
4. Send `Authorization: Bearer <token>` on every request, and send no Cuckoo
   cookie. Start with `GET /api/identity/session`.
5. On 401, delete the token and show sign-in again (signed out from the web,
   expired after 90 days, or the account was deleted). 503 means WorkOS or the
   database is unavailable: keep the token and retry.
6. Sign out with `POST /api/identity/devices/{device.id}/revoke`, then delete the
   token.

## QR library

The QR code is drawn on the server by the Rust `qrcode` crate 0.14.1 (MIT OR
Apache-2.0, `svg` feature only, no image dependencies), so the web bundle gains
nothing. The browser shows it as an `<img>` data URL, which cannot run script.
Checked by decoding a generated code back to its exact link with ZXing.

## Tests

- PostgreSQL (`cargo test --lib device -- --ignored`, `RELAY_TEST_DATABASE_URL`):
  `device_session_tests.rs` covers single use, replacement, expiry, concurrent
  redeems, caps, listing only your own devices and not signing out another
  person's; `device_http_tests.rs` covers the routes with a loopback WorkOS: a
  phone signs in and acts as its person without Origin, a cookie without Origin is
  still refused, a phone cannot make codes, forged tokens get 401, sign-out,
  expiry and a deleted WorkOS account end the phone session, and removal from a
  workspace cuts the phone off.
- Web: `web/tests/phone-sign-in.test.mjs` (countdown, expiry, scan detection,
  sign-out pending, failure and single submission, refused codes).

## Not built yet

- The mobile app itself, universal links and push registration (M1 and M4 in the
  mobile plan).
- A way to cancel a code before it expires (Hide only hides it).
- The plan's browser-based mobile sign-in (`/auth/mobile/start` with PKCE) is not
  needed for QR sign-in; it remains an option for people without the web open.
