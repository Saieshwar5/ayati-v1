# React client migration

The owner requested React on 2026-10-20 as Relay's forms, dialogs, permissions,
and shared interface state grew. React owns the client entry point and workspace
shell. This is a staged migration of the existing product, preserving its CSS,
server API contracts, authentication, and workspace boundaries.

## Converted

- `web/src/main.tsx`: React root, route selection and startup.
- `web/src/react/workspace-shell.tsx`: persistent quiet rail and branding.
- `web/src/react/workspace-application.tsx`, `web/src/navigation.ts`: same-workspace
  feature navigation with browser history and native identity/workspace changes.
- `web/src/react/chat/`: persistent workspace directory, channel/DM conversations,
  threads, creation dialogs, saved task-progress drawers and trusted approval references; owner-private transcripts retain the legacy bridge.
- `web/src/react/private-chat.tsx`: no-workspace private-agent fallback.
- `web/src/react/chat/composer.tsx`: shared React composer across channel, DM, thread
  and private-agent views. Private chat mounts an explicitly owned React island
  through `private-composer.tsx`; its legacy composer is removed.
- `web/src/react/account-menu.tsx`: native account popover and workspace switching.
- `web/src/react/account.tsx`: hosted sign-in entry, workspace chooser/creation,
  invitation acceptance and account view.
- `web/src/react/agents/`: selected team roster/profile workspace, source-scoped
  request progress and Stop, guided creation, profile editing, approved knowledge,
  permission drafts/previews/activation and preserved local-agent lifecycle controls.
  [The workspace contract](agents-team-workspace.md) describes supported tabs.
- `web/src/react/agents/automation/`: on-demand shared schedule list, explicit
  review/create, exact pause/resume and source-authorized run history. Reviewed
  connected-event triggers add signed source setup, filters, activation and bounded
  delivery history; provider subscription registration remains an operator step. Existing
  private-runtime automation keeps its separate compatibility boundary.
- `web/src/react/agents/rehearsals/`: on-demand draft-only sample dialog, saved job
  context/source review, guarded Start/Stop, personal history and shared reader.
  Ordinary agent guidance enters its normal prompt without granting tools.
- `web/src/react/settings/`: tabbed personal/workspace settings, appearance, general/activity,
  teams, setup, local accounts and Owner workspace connections/delegation. The legacy Settings screen and setup
  installer are removed; Agent details retains its existing connector integration.
- `web/src/react/people.tsx`, `member-details.tsx`, `member-access.tsx`: member and
  invitation lists, invitations, roles and explicit agent/App/panel grants.
- `web/src/react/approvals/`: current-access exact review, restricted inbox and
  approver/channel selection shared by workspace conversations and agent setup.
- `web/src/react/chat/organization-model.ts`, `pin-control.tsx` and
  `channel-lifecycle.tsx`: personal server pins and guarded archive/restore review.
  Existing conversation/thread composers retain drafts while closed to new sends.
- `web/src/react/chat/handoffs.tsx`: source-authorized parent/helper links, honest
  wait/outcome states and exact helper Stop in channel threads and Agents Work.
- `web/src/react/questions/`: compact saved references and typed ordinary-answer
  dialogs, guarded submissions, readable history and source revocation handling.
  Answers and business approvals retain separate authority.
- `web/src/react/results/`: bounded reply excerpts, saved-response reader and
  passive file/image viewing shared by workspace Chat/threads and an explicit
  private-runtime portal adapter. Legacy transcript DOM retains its existing owner.
- `web/src/react/ui.tsx`: shared React buttons, keyboard tabs and native dialogs.
- `web/src/react/requests.ts`: guarded mutations and reads that discard stale or
  unmounted responses. Mutations are user actions and never mount effects.

These screens use React state and JSX. Their old DOM implementations are removed.
React escapes identity and resource labels; no provider secrets enter the client.
Existing workspace-bound APIs remain authoritative for every access decision.

## Temporary boundary and remaining conversion

Home/panels, Apps/record frames, no-workspace agent management, private-agent transcripts,
connections, schedules, browser/approvals, memory/files and access-preview demo
still use their existing TypeScript implementations. `LegacyView` reserves an
empty `#app` subtree for them. React manages the shell but does not reconcile the
contents installed there. Each mount gets a `ViewLifetime` that owns its DOM
queries, bound agent API,
abort controller, timers, external listeners, portals and stream cleanup. React
Strict Mode remains enabled: disposed async imports cannot initialize, aborted
reads cannot publish into a new view, and remounts close the previous stream.
Legacy Chat is an explicit installer rather than a module-level side effect.
Artifact list state belongs to each chat installation rather than a module cache.

Feature links in the same workspace update browser history and the content area.
The rail/account shell retains its DOM identity. Private chat retains its React
agent list; the selected conversation mounts with bound requests and separate
user/workspace/agent drafts, attachments and reading position. Shared chat retains
its conversation sidebar and guards results with a selection version. Back/Forward
uses the same lifecycle boundary. Only same-origin same-workspace document links
are intercepted; workspace/account changes, downloads, modified clicks, auth URLs
and external links retain native navigation. Workspace/session revocation polling
continues and API permission checks remain authoritative.

An unavailable deployment chunk presents an explicit reload action. Keep old
hashed assets available during production rollouts; replacing local `web/dist`
while a page is open can require that reload. This does not justify automatic
reloads on ordinary feature or agent selection.

Next convert Home/panels and Apps, then private Chat with its contextual tools.
Keep each increment usable and
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

## Persistent navigation verification — 2026-10-20

Interaction regressions cover retained rail DOM identity, rapid feature changes,
Back navigation, removed pollers, persistent agent contacts, separate agent drafts,
closed streams, agent-bound request headers, ignored late replies and shared-chat
selection/draft isolation. The client suite, production build, browser-test type
check and source file limit pass. The previous global DOM contact implementation
was removed. Backend authentication, workspace authorization and runtime ownership
are unchanged by this client increment.
