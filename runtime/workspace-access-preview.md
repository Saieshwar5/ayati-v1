# Workspace access preview

Open `/?view=access-preview` on the local Ayati server, or follow Settings →
People and usage → Try the workspace access preview. The Vite client also serves
this route without a running Rust daemon.

This is an interactive UI fixture, not authentication. It does not send emails,
call WorkOS, create backend users, or authorize access to existing agents, Apps,
connections, conversations or files. Google sign-in is visibly unavailable.
The preview is restricted to localhost hostnames; this is a convenience boundary,
not a security mechanism. The existing local-owner API is unchanged.

## Try it

1. Continue with `owner@example.com` as a demo identity.
2. Create a workspace, then open People.
3. Create an invitation for `employee@example.com`.
4. Open the demo invitation. Switch accounts and enter the invited email.
5. Accept and inspect the member view. Sign out, return as the owner and remove
   the employee. Signing in as the employee no longer lists that workspace.
6. Create another workspace from the workspace switcher to try multiple memberships.

Workspace fixtures persist in this browser's local storage; the current demo
identity lives in tab session storage. Invitation links work only with the same
browser storage and origin. Anyone using this browser can impersonate any demo
email. Use invented addresses and never store real sensitive data here.
My account → Reset preview data removes the fixtures.

Invitations expire after seven days. Resend replaces the old invitation;
cancelled, expired and mismatched invitations are rejected in the fixture model.
Owners can appoint admins; admins can manage ordinary members. Owners cannot be
removed or demoted by this preview. Ownership transfer is intentionally not
implemented. These UI rules are not server-enforced authorization.

## Required before real users

Integrate a managed authentication provider and validate server-side callbacks,
sessions, recovery and revocation. Add durable workspace ownership and membership,
transactional invitations and audit events. Enforce access on every API, stream,
download and background execution path before connecting these screens to business
data. Verify provider behavior with development credentials, including wrong-account
invitations and owner recovery. Do not expose the local-owner server as a hosted
multi-user product.

## Validation

The client build/type check and all five client test files passed. New fixture
tests cover wrong-recipient, expired/cancelled invitations, duplicate acceptance,
removal followed by invitation replay, role restrictions and unrelated workspaces.
A local browser walkthrough verified owner onboarding, invitation creation,
wrong-account switching, direct employee joining and the read-only member list.
The repository source line-limit check passed. No live provider authentication,
email delivery or backend tenant isolation was tested or implemented.
