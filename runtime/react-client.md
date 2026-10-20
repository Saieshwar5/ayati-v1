# React client migration

The owner requested React on 2026-10-20 as Relay's forms, dialogs, permissions,
and shared interface state grew. React owns the client entry point and workspace
shell. This is a staged migration of the existing product, preserving its CSS,
server API contracts, authentication, and workspace boundaries.

## Converted

- `web/src/main.tsx`: React root, route selection and startup.
- `web/src/react/workspace-shell.tsx`: quiet rail, branding and feature navigation.
- `web/src/react/account-menu.tsx`: native account popover and workspace switching.
- `web/src/react/account.tsx`: hosted sign-in entry, workspace chooser/creation,
  invitation acceptance and account view.
- `web/src/react/people.tsx`, `member-details.tsx`, `member-access.tsx`: member and
  invitation lists, invitations, roles and explicit agent/App/panel grants.
- `web/src/react/ui.tsx`: shared React buttons, keyboard tabs and native dialogs.
- `web/src/react/requests.ts`: guarded mutations and reads that discard stale or
  unmounted responses. Mutations are user actions and never mount effects.

These screens use React state and JSX. Their old DOM implementations are removed.
React escapes identity and resource labels; no provider secrets enter the client.
Existing workspace-bound APIs remain authoritative for every access decision.

## Temporary boundary and remaining conversion

Home/panels, Apps/record frames, agent management, shared/private Chat, Settings,
connections, schedules, browser/approvals, memory/files and access-preview demo
still use their existing TypeScript implementations. `LegacySurface` reserves an
empty `#app` subtree for them. React manages the shell but does not reconcile the
contents installed there. Initializers run once after the shell commits, outside
React effects, so development Strict Mode cannot start duplicate streams/timers.

Feature and workspace links still perform full document navigation. This is
intentional: existing installers retain document-scoped subscriptions. Do not add
client-side route switching until the affected feature has explicit cleanup for
streams, listeners, timers, dialogs, drafts and pending requests. Workspace changes
must always discard the previous identity's active state.

Next convert Settings and agent management, then Home/panels and Apps, then shared
and private Chat with their contextual tools. Keep each increment usable and
exercise existing behavior before removing its compatibility implementation.
The application is not yet fully converted to React.

## Dependencies and development

Pinned runtime dependencies: React and React DOM **19.3.1** (MIT). Development
tooling: React type packages **19.3.0**, Vite React plugin **6.1.2**, and tsx
**4.23.15** for JSX-aware Node tests. Keep the existing Vite/TypeScript stack;
this increment adds no routing, UI-kit or global-state dependency. All resolved
dependencies are recorded in `web/package-lock.json`.

Use `npm ci --prefix web`, `npm run dev --prefix web`, and
`npm run build --prefix web` as before. The production identity server serves
`web/dist`; rebuild and reload to see changes. The existing development proxy is
for local-owner APIs; real identity still uses its registered same-origin server.

`npm test --prefix web` now loads tsx and runs both existing tests and native React
interaction tests in jsdom. Tests use React Strict Mode and `act`; real invitations
or permission changes are not required to test forms. `npm run check:e2e --prefix
web` checks browser-test types; it does not execute browser journeys.

## Validation

The first increment passes the production build, client tests, browser-test type
check, and repository file-length/whitespace checks. React tests cover duplicate
invitation sends, busy dismissal, explicit invitation acceptance, workspace-create
request IDs across failed retries, unauthorized remembered workspace selection,
role restrictions, grant failures, stale reads, safe identity text, keyboard tabs
and logout failure. Live UI checks are recorded in the implementation commit.

Sources: [incremental adoption](https://react.dev/learn/add-react-to-an-existing-project),
[effects and cleanup](https://react.dev/reference/react/useEffect).

## Navigation performance — 2026-10-20

A live Apps navigation measured approximately 1,084 ms in authenticated startup,
followed by a 551 ms Apps request. Startup made `/session` and `/api/context` in
sequence; both repeated sequential WorkOS session/profile checks. The identity
gateway also forced all built JavaScript and CSS to download on every navigation.

The session snapshot now includes an `adopted` hint on the explicitly bound
owner's membership, removing the context request for layout migration. Both live
WorkOS checks run concurrently on every protected request. This introduces no
authentication-result cache or relaxation of membership/resource authorization.
Only successful `/assets/<name>-<eight-character-hash>.js` and `.css` responses
receive private immutable caching. HTML, API responses and App documents retain
`no-store`. Changed builds use different asset URLs. Startup shows an accessible
status in place of an empty root, using the saved theme.

A subsequent live Apps navigation measured 324 ms in authenticated startup and
322 ms for its Apps request. These are local observations with network-dependent
provider timing, not latency guarantees. Temporary console timing instrumentation
was removed. Full document navigation remains until legacy installers have tested
teardown; this change does not introduce a client router or share identity state
between workspaces.

Regression coverage verifies concurrent provider checks, fresh revocation/email
validation, provider failures, cache-policy boundaries, one-request startup and
owner-only layout migration without overwriting existing layouts.
