# Local identity and workspace server

Run `cargo run -- identity` after building the client with `cd web && npm run build`.
Open `http://127.0.0.1:8765/`. It opens the client behind an authenticated workspace gateway; unscoped business APIs remain blocked. A process already
using the port must be stopped separately, or choose another `AYATI_PORT` and
register its corresponding callback with WorkOS.

## Configuration

Set `WORKOS_CLIENT_ID` and `WORKOS_API_KEY` privately in `.env`.
`WORK_OS_CLIENT_ID` and `WORK_OS_API_KEY` are accepted aliases. No credentials
belong in frontend variables or source control.

For the default local port, configure WorkOS development:

- Redirect URI: `http://127.0.0.1:8765/auth/callback`
- Initiate login URI: `http://127.0.0.1:8765/auth/login`
- Sign-out URI: `http://127.0.0.1:8765/`
- Enable the desired sign-in methods in AuthKit.

This mode serves its built client on the same origin as its backend; it does not
use the Vite origin at port 5173. No browser-to-WorkOS CORS permission is needed.
Use `127.0.0.1` consistently. `localhost` is intentionally not an alternate host.

## Implemented boundary

Hosted AuthKit performs authentication. Relay exchanges a code using server-held
credentials and PKCE, with an expiring, single-use state bound to an HttpOnly
cookie. Only verified email identities are admitted. Internal users are keyed by
provider user ID, never automatically merged by email.

The browser receives an opaque HttpOnly SameSite=Lax cookie. Only its hash is saved
in PostgreSQL. Local sessions last one hour; provider sessions are checked on each
protected request and revoked on logout. Provider access/refresh tokens are not
persisted. Provider failures deny access; existing cookies are not automatically
deleted on transient failure. The client can retry by reloading or sign in again.

PostgreSQL holds users, sessions, workspaces, memberships, invitations and audit
entries; `ayati identity` requires `RELAY_DATABASE_URL`.
Workspace creation and owner membership are transactional and request-idempotent.
Workspace IDs never confer access. Reads and writes check current membership.
Members cannot administer people. Admins can appoint other admins and change or remove
any membership while preserving at least one admin. See [Settings and ownership](settings.md).

Invitations are application-level WorkOS emails, not WorkOS organization grants.
After authenticating, a recipient explicitly accepts a pending Relay workspace
invitation. Acceptance requires the verified matching email, a confirmed delivery
response, an unexpired pending invitation, and still-valid inviter authority.
Relay membership is authoritative. Cancelling locally prevents acceptance even if
provider revocation fails. A removed member cannot replay an accepted invitation.
No workspace is auto-created for invited employees.

Uncertain email delivery is recorded as `delivery_unknown`, grants no access and
is not automatically retried. Cancel before reissuing. Emails may already have
been sent even when an API timeout occurs. Cancellation results disclose provider
revocation failure to the caller without restoring local access.

## Limits before a hosted pilot

This is loopback HTTP only. Production requires HTTPS/Secure cookies, deployment
hardening, appropriate retention and backup/restore, provider recovery and
step-up authentication acceptance, operational rate limits, and PostgreSQL
migration. Login attempts have a bounded in-memory pending-flow map; active
login flows do not survive server restart.

There is no workspace deletion, session-list UI, automatic token refresh,
or background email reconciliation yet. Invitation emails are generic application
invitations; the workspace name and offered role are shown after verified login.
Recipients on another computer cannot reach this localhost instance.

See [workspace client integration](workspace-client-integration.md) for the request boundary.
The original browser-only preview remains at `?view=access-preview` in the
regular client and is not used as authentication by this server.

## Validation on 2026-10-20

The configured API key returned HTTP 200 for a read-only WorkOS authentication
check. The hosted Google login completed through the real callback, and the owner
created a workspace. Its account, session and workspace survived a server restart.
The authenticated People screen showed the admin membership. A user-authorized
invitation was accepted by WorkOS for sending and appeared as pending locally;
the recipient subsequently accepted it, and the persisted Member membership was verified.

All 27 library tests passed with local socket access; six exercise the identity
module. Client build/type checking, five client test files, Rust library clippy
and the source line-limit check passed. Fixture tests cover membership denial,
invitation cancellation/expiry/replay, inviter authority changes, workspace
creation idempotency, session removal, PKCE/state binding and request guards.
These results do not establish production readiness or tenant-safe agent execution.

## PostgreSQL

All controller state lives in PostgreSQL. See
[local PostgreSQL setup and migration](postgres-workspaces.md).

## Display names

On first entry, including existing accounts without a name, Relay asks “What
should people call you?” before workspace selection. People can edit their
account-wide Display name in **My account** or under **Edit display name** in the
workspace chooser. Names are trimmed, required, and limited to 80 Unicode
characters; control characters are rejected. Names need not be unique.

The same name appears in account avatars, Users, chat recipients, direct-message
headings and message attribution, including older messages. Email remains visible
in account/member details and recipient selection to distinguish people with the
same name. Until someone chooses a name, existing messages retain the email
fallback. Identity, membership, grants and message ownership still use stable user
IDs; a name never grants access. Only the currently authenticated person can
change their name through `POST /api/identity/profile` with `{ "display_name": "…" }`.

PostgreSQL requires the explicit forward migration before starting the updated
server: run `workspace-db migrate` with the administrator connection (or
`./tools/local-postgres.sh migrate` for this local setup). No provider profile
write or email change is performed.
