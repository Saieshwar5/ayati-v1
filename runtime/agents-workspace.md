# Crisp Graphite Agents workspace

The workspace uses shared neutral light/dark tokens: white/near-black/graphite
in light mode and neutral charcoal surfaces in dark mode. Existing theme choice
and system preference remain supported. Home, Apps and Settings receive these
shared colors; their feature layouts are unchanged by this increment.

Agents has a contact-style sidebar with initials avatars, selected state,
approval counts, current task counts and a bounded saved-message preview. Search
and New agent remain visible. Desktop Agents can collapse the list using the
header control; mobile opens the existing chooser. Avatars are generated initials,
not uploaded images. Approval counts are not unread-message tracking.

`GET /api/agents` adds optional `activity.latest` with text (up to 160 Unicode
characters), task status and message creation time from that agent's private
store. Values are rendered as text. No schema migration or new dependency is
required. These remain local-owner workspace APIs, not new member authorization.

The conversation keeps its own scroll area above a centered floating composer.
Opening work controls does not remount the conversation or discard its draft.
Agent switching retains the existing full-page selection, scoped API/stream
identity and session-stored drafts. Browser zoom and narrow layouts keep the
composer inside the available workspace; very long input scrolls within it.

## Contextual work panels

Browser, Automation and Details are agent-header actions. Details links to Memory
and Files. At most one main work panel is open. Desktop panels sit beside chat;
up to 1000px they become full-screen native modal dialogs. Resizing an open panel
moves between these modes without resetting its content. Native focus trapping,
Escape, close and launcher focus return remain supported. Profile editing and
new-agent setup retain their focused dialogs.

Automation contains functional Schedules and an explicit Triggers placeholder.
Schedule creation, idempotent retries, pause/resume, dispatch state and run history
reuse [schedule controls](agent-schedules.md). The Details → Manage schedules
shortcut still opens the same Schedules tab. Trigger rules/sources are not added.

Browser opens the existing real guest view with Take control, Hand back and Close
browser session. Expand temporarily hides chat/list; Restore chat returns them.
Expansion is a layout action and does not change browser authority. Closing,
Escape or switching work panels clears the displayed frame and unsent private
input; it never resumes the agent or releases owner control. Reopen Browser to
continue or explicitly hand back. Escape belongs to the work panel rather than
being forwarded into the guest. No frames or private typing are saved in chat.

The work-panel coordinator handles exclusivity, responsive dialog presentation
and focus only. Runtime control, approval, schedule and memory authority continue
to use existing server paths. The UI does not introduce new permissions.

## Verification and remaining work

`tests/agents_workspace.mjs` covers exclusive panels, saved drafts and reading
position, collapsed sidebar, Automation keyboard tabs, explicit trigger placeholder,
responsive modal transitions, theme persistence, and 320/390px layouts. Existing
workspace/profile/schedule/tool/memory journeys remain relevant. The web-owner
case in `tests/browser_vm.mjs` checks real guest input and ownership when switching
panels, clearing unsent private text, expanding and restoring chat.

Custom avatars, unread-position tracking, drag-resizable panes, per-agent saved
pane widths, calendar recurrence, trigger management, Home custom panels and
agent-built Apps remain separate increments. No authentication/member work or
provider deployment is added. Test providers are synthetic; no paid inference
is needed for these checks.
