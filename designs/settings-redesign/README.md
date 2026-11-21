# Settings redesign (approved 2026-11-21, built)

Amends `designs/settings-simplified/` (D232). The owner reviewed the screens on the
design canvas and approved building them on 2026-11-21. The canvas is the editable
source: <https://claude.ai/artifact/GH2iG986T9xcn9MTTpVqUX> (private to the owner
until shared). Names and numbers on it are examples.

## What changed

| Before | After |
| --- | --- |
| You: Profile, Preferences, Help | You: Profile, Preferences |
| Workspace: Workspace, People, Connections, Company rules, Usage, Billing, Audit log | Workspace: General, People, Apps, Agent rules, Usage, Audit log |

- **Help** left Settings. It opens as "Help and feedback" from the account menu.
- **Billing**, **Export data** and **Email alerts** were placeholders and are gone
  until they are built. The workspace **colour** picker is gone: the client never
  used the colour.
- **Workspace** was split. General keeps the name, time zone and deleting the
  workspace. Agent rules holds what agents may do: company rules, models, the
  approval backup and Pause all agents.
- **Profile** has a large photo, grouped fields, the status and "Show when I'm
  online", and a live card showing how others see you.
- **People** and **Apps** are each one list. **Usage** leads with the budget and one
  charge. The **Audit log** is grouped by day.
- Link ids did not change (`section=workspace`, `connections`, `company-rules`), so
  existing links keep working; old Help and Billing links open Profile and Usage.

## One rule for saving

Typed text (a name, About, the workspace name) shows an "Unsaved changes" bar with
Discard and Save. Switches and choices save when they change. A choice with a
consequence (switching an app off, removing a model) asks first.

## Not on the canvas

Dark mode and phone layouts for the workspace pages were not drawn; the build uses
the existing theme tokens and the existing phone list-then-page behaviour.
