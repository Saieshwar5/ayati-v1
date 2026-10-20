# Coding practices

These instructions apply to the entire Ayati project. Read more specific
`AGENTS.md` files when working in a subdirectory.

## Understand the work first

- Before editing, understand the request, relevant code paths, existing
  conventions, dependencies, and available checks. Read nearby implementations
  and inspect Git status and the current diff when a Git repository exists.
- Resolve routine implementation choices using the project's conventions.
  Ask for clarification when missing information materially changes the behavior
  or scope; do not invent requirements.
- Treat `proj-docs` as reference material, not as code to change unless requested.
  Read relevant references selectively instead of scanning every reference repo.
- Use [the project index](proj-docs/README.md),
  [implementation status](proj-docs/implementation-status.md), the roadmap, and
  relevant decisions to recover context before building. Verify current behavior
  in code and checks; plans and old commit messages are not proof of implementation.
- When unsure about a design, inspect the relevant local references in
  `proj-docs/proj-ref/` or research current primary sources on the web. Resolve
  routine engineering questions this way without reopening settled product choices.

## Reuse useful open-source software

- Prefer existing software when it is stable, maintained, and makes Ayati simpler.
  Evaluate fit, license compatibility, dependencies, resource use, and integration
  effort. Popularity alone does not establish suitability or reliability.
- Reuse a focused library, CLI, or MCP integration where it solves the need.
  Keep Ayati's core small; adapt relevant ideas without importing an entire
  reference project's architecture or speculative features.
- For consequential dependency choices, verify the needed behavior with a small
  real integration check and record the reason, tested version, and material limits.
  See [development workflow](proj-docs/development-workflow.md) for examples.

## Use familiar, maintainable code

- Prefer widely used language idioms and established techniques. Use advanced
  or unusual language features only when they provide a clear benefit and fit
  the existing codebase. Avoid clever code that makes review or maintenance hard.
- Keep implementations direct. Do not add speculative features, unnecessary
  abstractions, or frameworks for hypothetical future requirements.
- Keep functions focused and make inputs, outputs, side effects, and ownership
  clear. Use explicit types at important boundaries when the language supports it.
- Keep source files at or below **500 lines**, including comments and blank lines.
  Split files by responsibility rather than moving arbitrary chunks or compressing
  code just to satisfy the limit.

## Organize for people and agents

- Group code by feature or responsibility, with clear boundaries between UI,
  application logic, storage, and external integrations where applicable.
- Give every file and directory a descriptive, searchable name that communicates
  its purpose. Follow the naming style of the surrounding codebase.
- Avoid ambiguous names such as `misc`, `stuff`, or numbered variants, and avoid
  catch-all utility files. Keep related implementation and tests easy to locate.
- Reuse existing behavior before duplicating it. Extract shared code when there
  is a concrete shared responsibility, not merely similar-looking lines.

## Build the React client efficiently

### Structure and migration

- Use React with TypeScript and the existing Vite setup for new client UI.
  Read [the migration boundary](runtime/react-client.md) and
  [the design system](runtime/design-system.md) before changing a screen.
- Migrate complete feature slices: components, state, requests and behavior checks.
  Preserve existing Home panels, Apps, chat tools and API contracts. Remove a
  replaced DOM implementation once its React replacement is verified.
- Keep feature components, hooks, types and tests easy to find together. Share
  established controls and request behavior; avoid a giant page component,
  catch-all hooks file or premature generic component framework. Write readable
  JSX rather than compressing complex markup to satisfy the file-length limit.
- React owns the DOM inside converted screens. Use refs for focus, scrolling,
  canvas and external integrations; avoid manually replacing React-owned markup.
  Keep legacy installers inside the documented compatibility subtree, initialized
  once outside React effects.
- Keep full document navigation until affected legacy features have explicit
  teardown. Introduce client-side routing only with tested cleanup and identity
  isolation; adding React alone does not make legacy screens safe to remount.

### State and server requests

- Keep temporary state local: search text, open dialogs, selected tabs and form
  drafts. Lift it only to the nearest common owner. Use reducers for coupled state
  transitions and narrowly scoped context for genuinely shared values; avoid one
  global context that rerenders the whole workspace on each keystroke or message.
- Separate server data, UI state and editable drafts. Derive filtered lists,
  counts and other computed values during rendering instead of duplicating them
  in state or synchronizing them with effects. Use stable record IDs as list keys.
- Reuse typed API/request boundaries. Fetch independent data concurrently, avoid
  duplicate polling and request waterfalls, and cancel or ignore stale reads.
  A response for an old user, workspace, resource or component must not replace
  the current view. Include those identities in any server-cache keys.
- Initiate mutations from explicit user actions, never rendering or mount effects.
  Guard duplicate submissions synchronously and show pending, success and failure
  states. Preserve failed form drafts. Reuse an idempotency key only for its exact
  payload; do not automatically retry uncertain writes without a server contract.
- Treat client permission checks as presentation. The backend must authorize
  every operation. Keep provider secrets out of client code and environment
  variables; use the existing HttpOnly session cookie rather than storing session
  tokens in JavaScript-readable storage. Handle denied/revoked access by clearing
  affected views. Discard identity-bound caches and subscriptions when switching
  accounts/workspaces, and scope persisted drafts by user/workspace.
- Start with existing hooks. When shared caching, deduplication or invalidation
  becomes substantial, evaluate a maintained query library rather than building
  a bespoke cache. Add state, form, routing or UI libraries for demonstrated needs,
  checking accessibility, compatibility and dependency cost before adoption.

### Effects and performance

- Keep rendering pure. Effects synchronize external systems, such as streams,
  timers and browser listeners; they are not a default mechanism for computing
  state or handling a button click. Declare their real dependencies.
- Every subscription, listener and timer needs cleanup. Abort or invalidate
  outstanding reads on teardown and identity changes. Keep Strict Mode enabled
  for converted components and fix duplicate effects instead of masking them
  with run-once flags. Apply the same ownership rules to portals and dialogs.
- Isolate frequent chat/stream updates to the affected feature. Preserve input
  focus, drafts and reading position when data refreshes; avoid remounting a whole
  screen or rewriting a transcript for each incoming token.
- Load feature screens and heavy tools on demand when it reduces startup work.
  Bound large datasets with server pagination or measured list virtualization;
  do not download/render an entire business history to show its latest entries.
- Measure an actual slow interaction with React Profiler and browser network/
  performance tools before optimizing. Use memoization for measured expensive
  work or required stable identities, not blanket `memo`, `useMemo` or
  `useCallback`. Check production bundle changes when adding substantial UI.

### Interaction and verification

- Reuse semantic controls, shared CSS tokens and established components. Support
  keyboard interaction, visible focus, accessible names, modal focus restoration,
  loading/empty/error states and the existing light/dark themes.
- Follow `runtime/design-system.md` as the current visual source of truth; older
  Floating Canvas prototypes are historical. Keep the grey quiet rail outside one
  rounded workspace frame and use the shared surface token inside every page.
  Chat's list and conversation share that surface. Use bordered Home panels/App
  tiles, flat Agents/Users rows, and spacing/dividers for Settings. Keep workspace
  switching and sign-out in the account menu; conversations belong in Chat.
- Preserve bounded work areas and intentional local scrolling. Check narrow and
  short windows, long labels and useful touch targets; avoid long page-sized forms
  and competing nested scroll regions. Keep existing theme tokens and features.
- Test user-visible behavior and important request boundaries. Cover role denial,
  double submission, failed saves, stale responses, unmount cleanup and identity
  changes where relevant. Use accessible roles/labels and React `act`; do not
  replace behavior tests with snapshots of component structure.
- Use the existing web check/build/test commands for implementation changes, then
  focused live browser checks for the changed journey. Browser-test type checking
  does not execute browser tests. Documentation-only changes need appropriate
  diff/link checks rather than rerunning unrelated application suites.

Follow React's guidance on [state structure](https://react.dev/learn/choosing-the-state-structure),
[effects](https://react.dev/learn/you-might-not-need-an-effect) and
[effect cleanup](https://react.dev/learn/synchronizing-with-effects).

## Test according to risk

- Verify changed behavior using the checks appropriate to the project, such as
  type checking, linting, builds, targeted tests, or a manual interaction check.
- Add meaningful tests for important logic, boundary conditions, failure paths,
  and regressions. Prioritize authentication, permissions, persistence, and
  external side effects when those areas change.
- Do not write tests for everything. Avoid tests that only mirror implementation
  details or add little confidence for a simple, reversible change.
- Run focused checks first. Run broader checks when the scope or risk warrants
  them. Report exactly what was verified and any checks that could not run.
- Run `./scripts/check-file-lines.sh` before finishing source-code changes when
  the local script is available. The ignored scripts may be absent in a fresh clone.

## Keep changes focused and safe

- Make the smallest complete change that solves the requested problem. Avoid
  unrelated refactors, formatting churn, or dependency upgrades.
- Validate data at system boundaries. Handle failures explicitly and give errors
  enough context to diagnose the problem without exposing secrets.
- Never hard-code credentials or log tokens, passwords, or sensitive data. Use
  environment variables or the project's existing secret-management mechanism.
- Before adding a dependency, check existing capabilities and weigh maintenance,
  compatibility, and size. Keep dependency lockfiles tracked when applicable.
- Update relevant documentation when commands, setup, configuration, or public
  behavior change. Explain why in comments when the code alone cannot communicate it.

## Follow good Git practices

- Preserve existing uncommitted work. Do not reset, discard, or overwrite changes
  unrelated to the task. Avoid destructive Git commands unless explicitly authorized.
- Use a focused branch for substantial changes once the repository is initialized.
  Keep commits cohesive and use messages describing the concrete change.
- Use Git history as working context: inspect recent commits and the history of
  relevant files before changing them. Use descriptive feature/milestone branches.
  Commit messages should preserve the problem, reason for the change, validation,
  and material remaining work, scaled to the size of the change.
- Keep meaningful implementation increments in focused commits. Preserve durable
  rationale in commit messages rather than relying only on a branch name or chat.
  Keep a short current status in `proj-docs/implementation-status.md` as building
  progresses, clearly separating working, tested, incomplete, and next work.
- Inspect the diff before committing. Stage only intended files and check for
  secrets, generated artifacts, and accidental reference-repository content.
- Respect `.gitignore`. Do not force-add ignored files unless the user requests it.
- Do not amend shared commits or force-push without explicit authorization.
  Publish commits or pull requests when requested by the user.

## Communicate the result

- Summarize what changed, why, and how it was verified. State material limitations
  or remaining work clearly instead of claiming unverified success.
- Call out behavior-changing tradeoffs early. Keep routine implementation choices
  autonomous and the final explanation concise.
