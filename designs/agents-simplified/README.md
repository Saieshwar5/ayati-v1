# Simplified Agents page (approved 2026-10-30)

Approved by the owner on 2026-10-30; building is deferred (D233). It replaces the Agents workspace in
`designs/agents-workspace/` (2026-10-26). It uses the Admin/Member roles and agent
managers from `designs/access-model/` (D231). The example company is fictional.

**One agent type.** "Who can talk to it" replaces Private and Workspace agents:
Everyone, Selected people, or Only me.

| File | Screen |
| --- | --- |
| `1-create-name-and-picture.png` | Create, step 1: name, job title, picture or upload |
| `2-create-instructions.png` | Create, step 2: one instructions box with starters and "Write it for me", "Add skills" (D234); locked Cuckoo rules |
| `3-create-people.png` | Create, step 3: who can talk to it, managers, who sees its activity |
| `4-create-tools-and-safety.png` | Create, step 4: connectors from the admin, Allow / Ask first / Block (stricter only) |
| `5-overview.png` | Overview: approvals waiting for you, this week, recent work, key facts |
| `6-activity.png` | Activity: numbers, requests per day, who asked, filters, timeline |
| `7-activity-details.png` | One request opened: every step, the exact fields approved, the reply |
| `8-instructions.png` | Instructions, the locked Cuckoo rules, skills it can use (D234), and what it knows |
| `9-tools.png` | Tools and safety for each action |
| `10-schedules.png` | Schedules and triggers |
| `11-environment.png` | Model picker; computer, browser, email, WhatsApp and phone marked "Coming later" |
| `12-phone.png` | Agent list, Activity and Create on a phone |

## Defaults (approved with the designs)

- New agents can talk to everyone.
- Instructions go in one box with a template.
- Owners, managers and admins see all of an agent's activity.
- Admins choose which models agents can use.
- Knowledge sits inside Instructions.
- Rules belong to the agent, not to one conversation.

Updated 2026-10-30: the skills additions above, and the main menu in the new
order: Home · Chat · Activity · Agents · Library · Settings.

## Removed

- Private and Workspace agent types.
- The separate model, channel-history, routine and successful-outcome fields.
- The 3-step permission setup and its work conversation.
- "Tools and approvals" as a second place for the same rules.
- "Try a sample task".
- The Environment placeholders shown as "Not connected".

`agents.fig` is the editable OpenPencil source.
