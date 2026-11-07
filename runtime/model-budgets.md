# Monthly model budgets (MG3, schema 89)

A budget caps what a workspace, or one agent, may be charged for models in a calendar
month. Months follow the workspace's time zone (UTC until an admin sets one). Charges
are the per-call charges from [model usage](model-usage.md). Built 2026-11-06.

## What happens

| Used this month | What people see | What Cuckoo does |
| --- | --- | --- |
| Below the warning share | "Warns at 80%." | Nothing |
| At or past the warning share (50 to 95%, default 80) | "Past 80% of the budget." in amber | Work continues |
| At the budget | "Used up: …" in red | New requests for the agent are refused with the reason, and the [gateway](model-gateway.md) answers every model call with 402 `budget_reached`. That covers work queued earlier and tasks that start on their own. |

A workspace budget covers every agent; an agent budget covers that agent. Whichever is
used up first stops the work. Raising or removing a budget lets work continue at once.

## Who does what

- `GET /api/identity/workspaces/{w}/model-budgets` (any member):
  `{budgets:[{agent, name, monthly_usd, used_usd, percent, warn_percent, state, month_start_ms}], can_change}`.
  The workspace budget comes first.
- `POST` the same path (admins) with `{agent, monthly_usd, warn_percent}`. `agent` is
  empty for the workspace. Amounts run from $0.01 to $1,000,000, and `monthly_usd: null`
  removes the budget. Each change goes in the Audit log, for example "Set the monthly
  model budget for Meera to $25.00".
- Settings → Usage lists budgets with a meter. Admins set them for the workspace or an
  agent, and remove them. Members see them read-only. The agent's Environment tab shows
  the budgets that limit that agent.

## Checks

- PostgreSQL (`postgres::model_budget_tests`):
  - Members can't set budgets, and invalid amounts are refused.
  - An agent at 85% is warned and keeps working. At 100% it's refused with the reason,
    while an agent without a budget keeps working.
  - A workspace budget stops every agent.
  - Removing or raising a budget restores work, and every change is audited.
- Real Goose (`tests/model_gateway.mjs`):
  - A request is queued while no executor runs, then the budget is used up.
  - When the executor starts, the request fails, and no call reaches the provider.
  - New requests are refused with the reason, and Settings → Usage shows "Used up".
- Web (`web/tests/model-budgets.test.mjs`): the warning and used-up states; one save per
  double click; a failed save keeps the draft; removal; members get no controls.

## Not yet

- Telling admins by notification when a budget passes its warning share (today it shows
  on Settings → Usage and the agent page).
- Budgets per person, and a hard per-request ceiling.
