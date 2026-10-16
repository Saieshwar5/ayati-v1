# Compact workspace interface

The root URL now opens Home. Use the persistent left rail for Home, Apps, Agents
and Settings. `/?agent=local-agent` and `/?agent=<saved-id>` still open the selected
conversation directly; `/?view=agents` also opens the default agent.

This implements the saved compact-rail layout direction, the contact-list and
optional work-panel interaction pattern, with Crisp Graphite neutral light/dark
surfaces replacing the original gradient shell. It uses the existing TypeScript/Vite frontend,
with no new package dependency or copied third-party branding/assets.

## Pages

- **Home:** compact Needs you rows, expandable exact approval details, recent
  terminal work, other active/queued requests and agent summaries. Requests waiting
  for an approval appear in Needs you without a second active-work row. An open
  review stays open across snapshot updates. Approval identity and server checks
  are unchanged. Recent work is bounded to six terminal requests per agent and
  eight across the workspace, sorted by request creation time; timestamps are
  labeled Requested rather than claiming an unrecorded completion time.
- **Agents:** searchable contact-style list, selected conversation, streaming
  replies, floating bottom composer and optional Browser/Automation/Details work
  panels. Details shows responsibilities, model, tools and settings editing.
  Automation exposes schedules; event triggers and business connector grants
  remain unavailable. Opening a panel preserves the selected agent and draft.
- **Apps:** an explicit empty state. App creation, installed business records and
  connectors are not simulated as working features.
- **Settings:** System/Light/Dark appearance, saved in this browser when local
  storage is available, plus the default agent's model/runtime and configuration
  readiness. Readiness is not proof of provider validity or a successful inference
  call. Credentials stay in local configuration; this UI never reads or edits them.
  Staff access, business connections and spending controls remain future work.

## Narrow screens and accessibility

The rail remains visible with short labels. Below 760 px, agent selection opens
in a modal drawer; Escape and Back to conversation restore focus. Details is a
nonmodal side panel on wide desktops and a modal at 1000 px or narrower. Resize
retains an open panel and its content while changing its display mode. Browser
and Automation remain header actions; Memory and Files are reached through Details.

Native dialogs provide modal focus containment. Navigation has an accessible name
and current-page indication; search has a label. All status text is retained rather
than conveyed solely by color. Semantic surface/text/focus/status tokens drive both
themes. Drafts remain isolated in tab storage per agent, and the composer resizes
when a restored draft wraps on a smaller screen. Existing unsupported file/media
behavior is unchanged. General history pagination and unread tracking remain work.

## Validation

`tests/workspace_ui.mjs` checks real local API data with a scripted provider:
root navigation, exact approvals, open-review preservation, panel/draft behavior,
search, theme persistence, narrow drawers and keyboard focus. Screenshots go to
`.test-runs/workspace/`. Existing Home recovery, two-agent VM and chat journeys
cover the retained runtime behavior. These are not paid-inference or hosted access
tests. Native mobile keyboards and a full screen-reader audit remain unverified.

Settings now includes read-only setup diagnostics and an explicitly consented
provider connection test. See [setup diagnostics](setup-diagnostics.md) for the
request limit, error behavior and distinction from agent/VM execution.

Agent Details now opens an editor for names, responsibilities and models. Edits require
an idle agent; role/model changes start fresh model context while preserving saved
history. See [profile editing](agent-profile-editing.md).

Agent settings now includes browser and memory switches for compatible environments.
See [tool controls](agent-tool-controls.md) for enforcement and preserved-data behavior.

Agent Details includes [Manage schedules](agent-schedules.md) for the selected
agent: create one-time/fixed-interval work, pause/resume and inspect saved run
outcomes. It does not create trigger rules or calendar recurrence.

The [Crisp Graphite Agents workspace](agents-workspace.md) replaces the earlier
gradient palette, adds a collapsible agent list and floating composer, and uses
one contextual Browser/Automation/Details/Memory/Files panel at a time. Existing
controls and per-agent drafts remain. Automation exposes Schedules and a clearly
labeled Triggers placeholder.
