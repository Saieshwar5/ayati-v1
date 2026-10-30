# Workspace access: invite-only early access

Implemented 2026-10-30 (schemas 41–42). Design: `designs/workspace-access/`.

- **Anyone may sign in** (WorkOS AuthKit; Google or email). A login alone reaches
  nothing: without a workspace or an invitation the person sees "not in a workspace
  yet" and can only enter an invite code or wait for an invitation.
- **Platform administrators** are the verified emails in `AYATI_PLATFORM_ADMINS`.
  They open **Cuckoo admin** (`/?view=account&admin=1`) to create, see and revoke
  workspace invite codes, and may create workspaces without a code.
- **A workspace invite code** (`CKO-XXXX-XXXX-XXXX`) creates one workspace, once,
  before it expires (7–90 days). It is shown once and stored as a SHA-256 hash; case,
  spaces and dashes do not matter when typing it. `POST /api/identity/workspace-codes/check`
  checks it without using it; `POST /api/identity/workspaces` with `code` creates the
  workspace and uses the code in the same transaction (concurrent use: one winner; a
  retry of the same request returns the same workspace). Every failure reads "Code
  invalid or expired"; ten failures in 15 minutes pause further attempts.
- **The creator is the owner** and invites people by email as before; invitees accept
  (or decline) on the workspace page after signing in with that email. No code is needed
  to join, and people may belong to several workspaces.

## Joining by invite link (schema 42)

- An owner invites an email in Settings → People. WorkOS emails the invitation, and the
  owner also gets a **Cuckoo invite link** (`/invite/<secret>`, shown once, stored as a
  hash) to send on WhatsApp or chat. **Copy new link** on a pending invitation replaces
  it; older links stop working. Links live as long as the invitation (7 days).
- The link always opens a Cuckoo page, signed in or not: who invited you to which
  workspace, the masked invited address and the expiry. Opening it never accepts it.
- Signing in from the page (`/auth/login?return_to=/invite/<secret>`, Google or email)
  returns to it. Only the invited email can **Accept and join**; anyone else sees whom
  the invitation is for and can switch account. Expired, withdrawn and unknown links
  explain what to do. After accepting: **Set up your profile** (name, optional job
  title), then the workspace opens; a used link offers "Open <workspace>".
- Routes: `GET /api/identity/invite-links/{secret}` (public preview),
  `POST …/{secret}/accept`, `POST …/{secret}/decline`,
  `POST /api/identity/workspaces/{id}/invitations/{invitation}/link` (managers).
- Sign-in failures are logged with a reason category only (no tokens or emails).

**WorkOS setting required:** in the WorkOS dashboard (each environment) set the
*Initiate login URL* to `https://<host>/auth/login`, under Applications → your
application → Redirects. Checked on 2026-10-31: staging sends visitors to
`https://app.dev.cuckoo.onl/auth/login` and accepts the
`https://app.dev.cuckoo.onl/auth/callback` redirect. To check it without the dashboard,
open the AuthKit domain directly and watch where it redirects. Staging has one such
URL, so invitation emails sent from local development also open the AWS host.

Admin routes: `GET/POST /api/identity/admin/workspace-codes`,
`POST /api/identity/admin/workspace-codes/{id}/revoke` (403 for everyone else).
Tests: `workspace_code_tests` (PostgreSQL), `web/tests/account.test.mjs`,
`tests/workspace_codes.mjs` (browser journey with screenshots).
