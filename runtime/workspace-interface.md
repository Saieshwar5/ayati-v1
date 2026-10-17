# Agent Studio update — 2026-10-17

[Agent Studio](agent-studio.md) supersedes the Agents layout described below: a
compact tinted contact sidebar, floating tool dock, contextual Artifacts panel
and bounded text attachments replace the masthead and hidden Files entry point.

---

# Personal Home update — 2026-10-17

The [Personal Home specification](personal-home.md) supersedes the Home hierarchy
and creation entry points described below. Panels come first; attention expands
from a compact strip, agents appear as contacts, and history is secondary. Home's
Ask Ayati drawer uses the real conversation. Panel/app creation chooses an agent
and opens an unsent chat starter. Apps activation is labeled Start using this app.
Existing permissions, private data and exact app preview isolation remain intact.

---

# Floating Canvas workspace interface

The root URL now opens Home. Use the persistent left rail for Home, Apps, Agents
and Settings. `/?agent=local-agent` and `/?agent=<saved-id>` still open the selected
conversation directly; `/?view=agents` also opens the default agent.

This implements the accepted [Floating Canvas design](design-system.md): compact
global navigation, a separate contact sidebar, raised Home/App surfaces and a
floating composer using neutral Graphite light/dark tokens. The workspace masthead
and redundant Home/Apps headings are removed. It uses the existing TypeScript/Vite frontend,
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
  Automation exposes schedules and event rules; Details exposes explicit business
  connector grants. See the capability documents for provider verification limits.
  Opening a panel preserves the selected agent and draft.
- **Apps:** searchable app launcher with active/preview state. Releases and access
  reveals preview, explicit record grants and archive controls. Scoped app previews,
  exact activation and ordinary records use the existing local Apps implementation;
  cloud hosting remains unimplemented. See [internal Apps](internal-apps.md).
- **Settings:** System/Light/Dark appearance, saved in this browser when local
  storage is available, plus the default agent's model/runtime and configuration
  readiness. Readiness is not proof of provider validity or a successful inference
  call. Credentials stay in local configuration; this UI never reads or edits them.
  Business connection controls use the current Composio integration. Staff access
  and spending controls remain future work.

## Narrow screens and accessibility

The narrow icon rail remains visible with accessible labels and hover titles.
Clicking Agents restores its separate contact sidebar. Below 760 px, agent
selection opens in a modal drawer; Escape and Back to conversation restore focus. Details is a
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
