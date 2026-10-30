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
  threads, creation dialogs, saved task-progress drawers and trusted approval references.
- `web/src/react/chat/composer.tsx`: shared React composer across channel, DM and thread
  views.
- `web/src/react/account-menu.tsx`: native account popover and workspace switching.
- `web/src/react/account.tsx`: hosted sign-in entry, workspace chooser/creation,
  invitation acceptance and account view.
- `web/src/react/home-panels/`: lazy personal workspace snapshots, exact draft review,
  source-safe detail readers, explicit agent requests and synchronized personal placement.
  Personal maintenance has explicit review/setup, guarded manual refresh, pause,
  fixed-interval scheduling and bounded snapshot health; it reuses the same source
  invalidation and view lifetime.
- `web/src/react/home-page.tsx`: Home with the business clock (device time),
  shared attention, saved panels and work records.
- `web/src/react/agents/`: selected team roster/profile workspace, source-scoped
  request progress and Stop, guided creation, profile editing, approved knowledge
  and permission drafts/previews/activation.
  [The workspace contract](agents-team-workspace.md) describes supported tabs.
- `web/src/react/agents/automation/`: on-demand shared schedule list, explicit
  review/create, exact pause/resume and source-authorized run history. Reviewed
  connected-event triggers add signed source setup, filters, activation and bounded
  delivery history; provider subscription registration remains an operator step.
- `web/src/react/agents/rehearsals/`: on-demand draft-only sample dialog, saved job
  context/source review, guarded Start/Stop, personal history and shared reader.
  Ordinary agent guidance enters its normal prompt without granting tools.
- `web/src/react/settings/`: tabbed personal/workspace settings, appearance, general/activity,
  teams and admin workspace connections/delegation.
- `web/src/react/people.tsx`, `member-details.tsx`, `member-access.tsx`: member and
  invitation lists, invitations, roles and explicit agent grants.
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
- `web/src/react/results/`: soft bounded reply boxes with inline corner-arrow
  expansion, a shared source-checked saved-response lifecycle, full reader and
  passive file/image viewing shared by workspace Chat and threads.
- `web/src/react/ui.tsx`: shared React buttons, keyboard tabs and native dialogs.
- `web/src/react/requests.ts`: guarded mutations and reads that discard stale or
  unmounted responses. Mutations are user actions and never mount effects.

These screens use React state and JSX. Their old DOM implementations are removed.
React escapes identity and resource labels; no provider secrets enter the client.
Existing workspace-bound APIs remain authoritative for every access decision.

## Navigation

The whole workspace client is React. The single-user daemon's DOM screens (Apps,
private agent chats, local agents, local connections and setup, schedules, browser,
memory and the access-preview demo) were removed on 2026-10-29 together with the
owner runtime they called. Links to `?view=apps` or `?agent=` now open Home.

Feature links in the same workspace update browser history and the content area.
The rail/account shell retains its DOM identity. Shared chat retains its
conversation sidebar and guards results with a selection version. Back/Forward
uses the same lifecycle boundary. Only same-origin same-workspace document links
are intercepted; workspace/account changes, downloads, modified clicks, auth URLs
and external links retain native navigation. Workspace/session revocation polling
continues and API permission checks remain authoritative. Outside a workspace the
client shows the account page (sign-in and workspace chooser).

An unavailable deployment chunk presents an explicit reload action. Keep old
hashed assets available during production rollouts; replacing local `web/dist`
while a page is open can require that reload. This does not justify automatic
reloads on ordinary feature selection.

## Dependencies and development

Pinned runtime dependencies: React and React DOM **19.3.1** (MIT). Development
tooling: React type packages **19.3.0**, Vite React plugin **6.1.2**, and tsx
**4.23.15** for JSX-aware Node tests. Keep the existing Vite/TypeScript stack;
this increment adds no routing, UI-kit or global-state dependency. All resolved
dependencies are recorded in `web/package-lock.json`.

Use `npm ci --prefix web`, `npm run dev --prefix web`, and
`npm run build --prefix web` as before. The production identity server serves
`web/dist`; rebuild and reload to see changes. Real identity uses its registered
same-origin server.

`npm test --prefix web` now loads tsx and runs both existing tests and native React
interaction tests in jsdom. Tests use React Strict Mode and `act`; real invitations
or permission changes are not required to test forms. Connected browser journeys
live in the repository `tests/` directory (see `tests/README.md`).

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

The session snapshot then included an `adopted` hint (removed with installation
adoption on 2026-10-29), removing the context request for layout migration. Both live
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
