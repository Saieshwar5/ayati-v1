# Simplified Settings (approved 2026-10-30)

Replaces the tabbed Settings in `designs/settings/` (option B, 2026-10-22). Uses the
Admin and Member roles from `designs/access-model/` (D231). Approved by the owner on
2026-10-30. Building is deferred and follows the access model.

One left sidebar with two groups replaces the two rows of tabs:

| Group | Member sees | Admin sees |
| --- | --- | --- |
| You | Profile, Preferences, Help | Profile, Preferences, Help |
| Workspace | This workspace (read-only) | Workspace, People, Connections, Usage, Billing, Audit log |

Usage and Billing are shown with a "Coming later" badge. They are not built yet.

| File | Screen |
| --- | --- |
| `1-member-profile.png` | Photo, name, job title, About; sign-in email read-only |
| `2-member-preferences.png` | Theme and notification defaults on one page |
| `3-member-this-workspace.png` | Who the admins are, what admins can see, Leave workspace |
| `4-admin-workspace.png` | Name, colour, time zone, approval backup delay, models; delete with a 7-day undo |
| `5-admin-people.png` | People and invitations, Admin/Member roles |
| `6-admin-connections.png` | Connectors, external MCP servers and computers |
| `7-admin-usage.png` | Spend per model by day, top agents, a table by model (example numbers) |
| `8-admin-billing.png` | Plan, this month, payment method, invoices: empty during early access |
| `9-admin-audit-log.png` | Audit log: readable changes with filters |
| `10-help.png` | Report a problem to the Cuckoo team, plus short guides |
| `11-phone.png` | The sidebar becomes a list; each item opens its page |

These leave Settings:

- Access: who can use an agent moves to each agent.
- Teams: the simulation results move to the simulation page.
- Agent creators: everyone can create agents.
- The internal bug tracker: replaced by Report a problem.

Signing out and switching workspace stay in the account menu.

The Usage chart colours are the validated categorical slots `#2a78d6`, `#eb6834` and
`#1baf7a`. The aqua slot has low contrast against white, so the screen pairs the chart
with a legend and a table of exact values. Every name and number in these screens is
an example.

Updated 2026-10-30 to match later decisions:

- **Audit log:** the admin "Activity" page is renamed "Audit log" (D235), so it is
  not confused with the main-menu Activity page.
- **Models:** the Workspace page has a "Models" setting for the default model and
  which models agents may use (D233).
- **Main menu:** the rail shows the new order: Home · Chat · Activity · Agents ·
  Library · Settings.

`settings.fig` is the editable OpenPencil source.
