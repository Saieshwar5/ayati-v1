# Cuckoo workspace design system

Current accepted direction, 2026-10-22. This is the authoritative visual and
interaction guide for the existing Cuckoo client. It supersedes the older
Floating Canvas, floating dock, tinted Chat directory and agent-first Home
layouts. Historical prototypes explain past choices; they are not current rules.
Feature specifications still govern permissions, persistence and execution.

## Shared workspace shell

- Keep the quiet global navigation rail visible across Home, Chat, Agents,
  Library and Settings. It sits on the light grey outer canvas, without its own
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
  every container or add boxes around Agents or Users rows. Ordinary agent prose
  uses the softly filled response box described under Chat.
- Preserve filled primary actions, borderless icon buttons, soft search fields
  and selected-row fills. Reserve stronger focus emphasis for interaction:
  ordinary controls keep the shared 2 px focus outline; the composer uses its
  1 px border plus a 1 px focus ring without changing layout. In forced colors,
  give the composer a system-color outline because shadows are suppressed.
- Keep Home panels and app tiles individually bordered. Use flat rows for people
  and agent directories; do not wrap every entry in a separate rounded card.
- Keep selection and hover/focus highlights pale grey. Primary actions are dark.
  Status colors are small and meaningful, always paired with readable text.
- Keep interaction states explicit: hover applies only to unselected navigation
  items; selection keeps its fill while hovered. Rail links and buttons keep the
  shared 2 px keyboard focus outline inside the control (offset -3 px), avoiding
  an outer capsule. Selected navigation uses the contrasting on-accent color;
  forced colors uses Highlight. Focus preserves icon geometry and selection.
  Chat rows use the soft fill for hover and the selection fill for the current
  conversation, retaining their inset keyboard outline.
- Profile avatars in Agents use `agent-profile-avatar` inside shared `ui-avatar`
  controls. Legacy contact positioning/status-dot rules belong to
  `.agent-card > .agent-avatar`; do not apply them to profile or editor avatars.
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
they cannot be removed, resized or reordered. The clock shows device time until a
shared business location setting exists. The clock has a subtle soft fill; notifications and
custom panels use white bordered surfaces with clear titles and consistent gaps.

Do not restore the agent/user directory, agent presence row, large prompt hero or
fictional business metrics. Agents has its own management page; people management is under Settings.
A short first-panel empty state and Add panel are sufficient when no panels exist.
Customize reveals arrangement controls; Done returns to normal reading.

Notifications previews existing scoped agent attention: approvals, problems and
results, with at most three rows. View all opens the existing inbox. Counts are
pending requests/outcomes, not unread messages. User mentions and chat notification
sources are not implemented by this styling work. Do not fabricate them.

Preserve saved panels, supported templates, review, grants, maintenance and
removal. Add panel chooses an existing agent and opens
an unsent creation flow. Placement remains browser-local where implemented;
managed workspace panels synchronize personal placement on the server. Their
[update agreements](home-panel-maintenance.md) name the responsible agent and show
concise freshness/health. Keep full source arguments and update setup in the
on-demand dialog; preserve enough space to read the full panel value. See
[workspace Home panels](workspace-home-panels.md) and
[Home attention](home-attention.md) for the underlying contracts.

## Chat

Align the signed-in person's messages to the right and other people's and agents'
messages to the left, including thread replies. Determine ownership by user ID,
never by display name or who requested an agent. Use a subtle themed fill for own
messages, retain author/Agent labels, and keep message text naturally aligned.

Use one persistent Chat shell for Channels and Direct messages, including people
and agents. The conversation list and message area share the same surface color,
with a thin vertical divider. The global rail remains outside the shared frame.
Selecting a conversation changes only the right pane; keep the list and workspace
navigation mounted. Searching must preserve the active transcript, its reading
position and scoped drafts. Reject stale responses.

Use the approved OpenPencil page 09 / option 1: one workspace with a floating
composer. The desktop Chat sidebar is 280 px wide; intermediate windows use
240 px before the existing phone list/conversation switch. Sidebar and conversation
headers share a 64 px height. Only the conversation header has a bottom divider;
do not extend it under the sidebar's Chat title. Keep connection status in the
header. Use 28 px single-line sidebar rows, 40 px header avatars and 36 px
incoming-message avatars. Unread rows use a bold name and an accent count.

People and agents share the same lists and the same round avatars (`faces.tsx`).
People show their photo or initials on a tinted circle; agents show an uploaded
image or a bundled robot (`web/public/agent-avatars/robot-01.svg` to `robot-24`).
An agent is told apart only by the small `AI` tag after its name — never by a
separate sidebar section. Private channels show a lock and public channels `#`.
Names and avatars open a profile drawer; People & agents at the foot of the list
opens the whole-workspace directory (everyone sees every member).

The search button beside Chat opens a focused dialog for authorized channels,
people and agents, with category filters and name/email matching. This is directory
search, not message-history search. Typing and filtering do not write. Choosing
an existing direct conversation reopens it; a result labelled Start conversation
uses the same guarded creation operation as New message. Keep failed queries,
block duplicate submissions and ignore late navigation after closing. New message is the labelled plus beside Direct
messages; Create channel is the labelled plus beside Channels. Both have accessible
names and hover labels. Escape closes search and restores the trigger's focus;
arrow keys move between the query and results, with native keyboard activation.

Keep channel, person and agent headers slim. Use the existing shared composer,
centered on a reading column up to 760 px, with a control border, 18 px corners and
a restrained shadow. Keep it in layout flow with a bottom inset; long messages,
attachments and voice controls must reserve transcript space rather than cover
messages. Keep own time and delivery indicators together on a predictable metadata
line; omit the repeated own name/avatar. Retain visible reply counts and reveal
ordinary reply actions on hover/focus, keeping them available on touch devices.
The message toolbar sits beside a message on hover or focus (on its top edge on
phones): three quick reactions, a reaction picker, reply in thread and More.
More offers Edit and Delete only on your own messages, plus Copy link. Reactions
are a fixed set of twelve, shown as chips under the message; they never approve
or request work. Edits show "edited"; deleting asks first, leaves "Message
deleted" in place and stops agent work the message started that has not finished.
Stop recording attaches the audio to the draft; Send remains explicit. Recipient selection is under
@; typing an @name alone does not dispatch an agent. Private-agent tools remain
in the header overflow menu. Do not restore the permanent Create panel button
above the composer; explicit creation-mode links and Home creation still work.

At phone widths show either the list or selected conversation with a predictable
Back action. Preserve attachments, approvals, history, streaming and active-work
Stop where supported. Closing a tool does not hand back browser control or resume
work. See [Unified Chat](unified-chat.md) for current channel/thread capabilities,
access boundaries and remaining work; a visual component grants no authority.

Personal Pinned rows appear once above Channels and Direct messages, in pin-time
order. Pin controls are keyboard/touch accessible and keep unread/selected state.
Archived channels leave active pins and lists; an explicit Archived section keeps
them reachable with their source permissions. Show closed intake in the root view
and Archived thread title. Archive review belongs near the top of Details and
shows visible unfinished work. Restore opens future intake only. See
[Chat organization](chat-organization.md).

Agent prose uses the selected OpenPencil corner-arrow design: a soft themed fill,
quiet border and 16 px corners. Long replies have a bounded deterministic excerpt,
a fading lower edge and a bare corner-arrow icon with a 44 px target. No labelled
expansion footer or character count. The icon opens the saved body inline and
reverses to collapse; expanded replies also have a top corner collapse control.
Short replies use their natural height. On phones, place the box below its author
row to use the available width. Keep conversation scrolling and preserve drafts.

Remove Check progress launchers from Chat; request activity remains in Agents Work.
Keep ordinary reply status, Stop, questions and approval controls. Expanded saved
replies retain exact Copy/Markdown actions and a secondary full reading view.
New saved versions load explicitly so reading does not jump. Keep images as small
thumbnails until opened in a fit/zoom viewer. See [saved reading](result-reading.md)
for source authority, supported files and current limits.

## Agents and people management

Agents follows the selected **B / Agent workspace** concept: a searchable flat
team roster beside the selected agent's profile, with Overview, Work, Knowledge,
Access, Automation, Environment and Activity tabs. Keep rows quiet, actual status
and purpose readable, and work separated with spacing/dividers. On phones show
the roster or selected profile with an explicit return control. Profile editing
retains the existing drawer. Conversations live in Chat; Open conversation from
work returns to its exact source/thread. Unsupported capabilities must say so.
See [behavior and authority](agents-team-workspace.md).

Settings → Workspace → People uses the same directory language. People and invitations remain separate
tabs; inviting opens a focused dialog. Details opens the member drawer. Keep
workspace membership, roles and agent access distinct. Preserve
explicit saves, pending states, validation and confirmation for destructive actions.

## Settings

Settings follows approved Option B: everyone sees Personal; only Owners see the
Workspace scope. Use a compact segmented scope control followed by flat category
navigation and one bounded, locally scrolling detail area. Personal contains My
account, Appearance and Help & feedback. Workspace contains General, People,
Access, Connections, Activity and Teams. Hide owner-only sections for
Users and enforce the same boundary on the server. Use Owner/User role labels.
Keep all workspace administration here rather than adding Users, Company or
Feedback global navigation items. Group controls with headings, spacing and thin
separators. Keep the current theme and setup/connection behavior. See
[Settings and ownership](settings.md) and [approved mockups](../designs/settings/README.md).

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

The 2026-10-22 Chat option 1 implementation passed the production build, all 65
frontend tests (including five focused search/direct-opening regressions), browser-
test type checking, source line limits and diff checks. Live local Chromium checks
cover channel/person/shared-agent views, search categories and keyboard dismissal,
existing-DM reuse, retained drafts/reading position, growing composers, thread
opening, light/dark themes, short windows, widths down to 320 px and forced colors.
Six additional private-agent adapter checks used provider fixtures because the
sample workspace has no legacy private agents. These establish UI behavior only;
no LLM calls or external integrations were exercised. Screenshots were reviewed.
