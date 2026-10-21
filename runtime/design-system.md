# Cuckoo workspace design system

Current accepted direction, 2026-10-21. This is the authoritative visual and
interaction guide for the existing Cuckoo client. It supersedes the older
Floating Canvas, floating dock, tinted Chat directory and agent-first Home
layouts. Historical prototypes explain past choices; they are not current rules.
Feature specifications still govern permissions, persistence and execution.

## Shared workspace shell

- Keep the quiet global navigation rail visible across Home, Apps, Chat, Agents,
  Users and Settings. It sits on the light grey outer canvas, without its own
  capsule, enclosing border or shadow. The selected feature uses a graphite tile.
- Put the entire work area inside one rounded surface with shadow-led separation
  from the canvas. This supersedes the visible outer-border treatment. Use a
  transparent 1 px border to preserve content geometry. Keep the 12 px desktop
  shell inset and 18 px frame radius; phones use a 6 px inset and 14 px radius.
  Light mode uses a soft downward shadow (0 8px 24px, ink at 10%); dark mode uses
  black at 32%. At widths up to 600 px, reduce the shadow to 0 4px 12px. Use the
  dedicated workspace shadow tokens so buttons and overlays retain their own depth.
- Use the same white surface inside the frame across pages. Grey outside means
  global navigation; white inside means the current feature. The shared surface
  provides elevation across Home, Chat, Agents and all other workspace pages.
  Use the selective inner-border hierarchy below; avoid nested elevated page containers.
- Keep workspace switching, account identity and sign-out inside the bottom
  account menu. Do not restore a permanent email/workspace/sign-out header.
- Use Cuckoo's approved bird/C mark and lowercase wordmark. Ayati remains an
  internal repository and runtime name; do not replace internal identifiers as a visual cleanup.

## Color, hierarchy and components

Use semantic tokens from `web/src/workspace.css` rather than hard-coded white or
new parallel palettes. `--canvas` is the outer background; `--surface` is the main
work area; `--soft` is a subtle inset/search/hover fill; `--selection` marks selected
items. Dark mode uses the corresponding existing tokens, not literal white.

- Establish hierarchy with spacing, type, alignment and thin dividers first.
- Apply the approved OpenPencil D / Selective treatment: keep ordinary row,
  table and section dividers on `--border` (#E5E7EA in light mode), use
  `--border-panel` (#D1D6DD) for Home panels and App tiles, and
  `--border-control` (#AEB8C5) for composers, outlined buttons and form controls.
  The Chat directory boundary uses `--border-structure` (#D6DAE0). Each has a
  corresponding dark-theme value. Keep resting borders 1 px; do not thicken
  every container or add boxes around messages, Agents or Users rows.
- Preserve filled primary actions, borderless icon buttons, soft search fields
  and selected-row fills. Reserve stronger focus emphasis for interaction:
  ordinary controls keep the shared 2 px focus outline; the composer uses its
  1 px border plus a 1 px focus ring without changing layout. In forced colors,
  give the composer a system-color outline because shadows are suppressed.
- Keep Home panels and app tiles individually bordered. Use flat rows for people
  and agent directories; do not wrap every entry in a separate rounded card.
- Keep selection and hover/focus highlights pale grey. Primary actions are dark.
  Status colors are small and meaningful, always paired with readable text.
- Use existing shared buttons, search fields, tabs, drawers and native dialogs.
  Avoid underlined link-like styling for primary actions and bulky inline forms.
- Keep title/action rows compact. Home needs no repeated Home heading or tagline;
  management pages can use a concise title, search and one primary action.
- Use comfortable text sizes and modest padding. Avoid oversized empty headers,
  decorative shadows, excessive nesting or shrinking text to make a layout fit.
- Use shadows for the shared workspace and sparingly for overlays. Keep flat rows
  and ordinary panels grounded. Hover and keyboard focus should both reveal useful
  emphasis; essential actions must remain available on touch screens.

## Home

Saved custom panels are the main content. A permanent Business clock and
Notifications row precedes them. Both standard panels remain outside Customize:
they cannot be removed, resized or reordered. The business location/timezone uses
the existing owner editor. The clock has a subtle soft fill; notifications and
custom panels use white bordered surfaces with clear titles and consistent gaps.

Do not restore the agent/user directory, agent presence row, large prompt hero or
fictional business metrics. Agents and Users have their own management pages.
A short first-panel empty state and Add panel are sufficient when no panels exist.
Customize reveals arrangement controls; Done returns to normal reading.

Notifications previews existing scoped agent attention: approvals, problems and
results, with at most three rows. View all opens the existing inbox. Counts are
pending requests/outcomes, not unread messages. User mentions and chat notification
sources are not implemented by this styling work. Do not fabricate them.

Preserve saved panels, supported templates, review, grants, maintenance, removal,
Recent activity and active-work Stop. Add panel chooses an existing agent and opens
an unsent creation flow. Placement remains browser-local where implemented;
server-synchronized placement in the Home plan is still future work. See
[Home panels](home-panels.md), [organized Home](organized-home.md), and
[Home attention](home-attention.md) for the underlying contracts.

## Chat

Align the signed-in person's messages to the right and other people's and agents'
messages to the left, including thread replies. Determine ownership by user ID,
never by display name or who requested an agent. Use a subtle themed fill for own
messages, retain author/Agent labels, and keep message text naturally aligned.

Use one persistent Chat shell for Channels and Direct messages, including people
and agents. The conversation list and message area share the same surface color,
with a thin vertical divider. The global rail remains outside the shared frame.
Selecting a conversation changes only the right pane; keep the list, search and
workspace navigation mounted. Preserve scoped drafts and reject stale responses.

The pencil action means New message. The plus beside Channels means Create
channel. Both need accessible names and clear hover labels. Do not use two
indistinguishable plus buttons for these different actions.

Keep channel, person and agent headers slim. Use a compact single-row floating composer with an inset and subtle shadow. It grows
for long messages and attachments, reserving transcript space above it. Keep own
message time and delivery indicators compact; omit the repeated own name/avatar.
Stop recording attaches the audio to the draft; Send remains explicit. Recipient selection is under
@; typing an @name alone does not dispatch an agent. Private-agent tools remain
in the header overflow menu. Do not restore the permanent Create panel button
above the composer; explicit creation-mode links and Home creation still work.

At phone widths show either the list or selected conversation with a predictable
Back action. Preserve attachments, approvals, history, streaming and active-work
Stop where supported. Closing a tool does not hand back browser control or resume
work. See [Unified Chat](unified-chat.md) for current channel/thread capabilities,
access boundaries and remaining work; a visual component grants no authority.

## Agents and Users

Agents manages configuration; conversations live in Chat. Use a searchable flat
list with avatars, names, roles/descriptions and actual status, separated by thin
lines. Hover and focus-within give a subtle highlight. Manage opens the existing
configuration drawer with Overview, Capabilities and Advanced sections.

Users uses the same directory language. Members and invitations remain separate
tabs; inviting opens a focused dialog. Details opens the member drawer. Keep
workspace membership, roles, agent access and App/panel grants distinct. Preserve
explicit saves, pending states, validation and confirmation for destructive actions.

## Apps and Settings

Apps uses bordered launch tiles: icon, name, short description, actual state and
an obvious Open or Preview action. Use the app identity rather than unrelated card
colors to distinguish entries. Give tiles subtle hover/focus border emphasis.
Manage holds release and access controls. Opening an app retains the existing
large sandboxed surface; preview and active records stay isolated. Activation
still applies to the exact reviewed release.

Settings shows one category at a time. Group controls with headings, spacing and
thin separators rather than another enclosing card. Preserve the existing tab
navigation, connection/setup flows and theme controls.

## Implementation and verification

Follow [React client boundaries](react-client.md) and `AGENTS.md`. Refine the
existing client instead of rebuilding its features. React owns converted screens;
legacy controllers remain within their documented compatibility subtrees.

- Keep the shell stable during feature navigation. Workspace/account changes
  still discard old identity-bound state through full document navigation.
- Bound work areas to the viewport and use intentional local scrolling. Avoid
  document-wide horizontal overflow, stacked full-height lists and hidden composers.
- Preserve workspace content clipping at rounded corners while allowing the shell
  shadow outside the surface. Check that the visible shadow is not abruptly cut
  off and that menus, dialogs and sidebar controls remain reachable. Forced-colors
  mode uses a system-color frame border instead of relying on the shadow.
- Check desktop, short windows and narrow screens down to 320 px, long labels,
  keyboard focus, touch targets, dialogs and theme tokens. Respect reduced motion.
- Use native semantics, accessible names, visible focus, Escape dismissal and focus
  restoration. Preserve failed drafts and explicit loading/empty/error states.
- Run proportionate build/type and behavior checks. Layout checks do not establish
  live provider behavior, backend completeness or deployment readiness.

The 2026-10-20 white-surface implementation passed the production client build, all
11 client test files, source line limits and light-mode browser checks across Home,
Apps, Agents, Users and Settings at desktop, 400 px and 320 px widths. Dark-mode
tokens remain in use; these checks did not include a full dark-mode visual audit.

The 2026-10-21 shadow-led refinement passed the production client build, all 60
frontend tests across 17 files, browser-test type checking, source line limits and
diff checks. Twenty-eight live local Chromium checks covered eight workspace pages
in light/dark mode, unchanged shell geometry, explicit/system themes, chat draft/
focus/reading-position preservation, menus/dialogs, short windows, 400/320 px widths
and forced-colors fallback. Screenshots were visually reviewed. These checks used
the local scripted company workspace; they did not invoke an LLM or external tools.

The 2026-10-21 D / Selective inner-border refinement passed the production client
build, all 60 frontend tests, browser-test type checking, source line limits and
diff checks. Local browser checks verified selective hierarchy on Home, Chat,
Agents and Users; shared shell/theme behavior across eight pages; focus/draft/
scroll preservation; dialogs/menus; short and narrow layouts down to 320 px; and
composer focus in forced colors. Screenshots were reviewed with no browser errors.
The scripted sample workspace had no custom Home panels or Apps: their updated
border rules were inspected in source, not verified against populated records.
