# Workspace access: invite-only early access

Implemented 2026-10-30 (schema 41). Design: `designs/workspace-access/`.

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

Admin routes: `GET/POST /api/identity/admin/workspace-codes`,
`POST /api/identity/admin/workspace-codes/{id}/revoke` (403 for everyone else).
Tests: `workspace_code_tests` (PostgreSQL), `web/tests/account.test.mjs`,
`tests/workspace_codes.mjs` (browser journey with screenshots).
