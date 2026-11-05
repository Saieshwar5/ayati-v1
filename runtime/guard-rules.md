# Company rules: the Guard, first part (schema 64)

Company rules are conditions every agent must follow on top of each action's
Allow / Ask first / Block ([agent tools and approvals](tools-and-approvals.md)).
They come from the core-agent plan (CA1, draft decision D239). Built 2026-11-05.

## What a rule is

One condition on one reviewed connector action, for example on Shop's
`refund.create`:

| Kind | Checks | Read-back |
| --- | --- | --- |
| Each request | One field of the exact request: over, at least, under, at most (numbers); is, is not (text, numbers, yes/no) | "Refund an order: if amount is over 2000, ask first." |
| Today's total | This request's amount added to the same action's total since the start of the workspace's day | "Refund an order: if today's total amount would go over 10000, block it." |

The effect is **Ask first** or **Block**. A rule can only make an action stricter:
block wins over ask, and an action the agent's rule blocks stays blocked.

The read-back is made by code from the saved rule (`guard::summary`), never by a
model. The editor shows the same words before saving
(`web/src/react/settings/company-rules-model.ts`).

## How a request is checked

`control_propose` runs the Guard inside the action's own transaction, after the
agent's rule is found and before anything is sent:

1. Every enabled rule on this action (and account, if the rule names one) is
   evaluated against the exact request fields.
2. **Unsure means hold.** If the field is missing or has the wrong type, a person
   decides (ask first), even for a block rule.
3. **Totals reserve.** Requests waiting for approval, executing, completed or
   uncertain count towards today's total; denied, rejected and invalidated ones don't.
   The day starts at midnight in the workspace's time zone (UTC when unset).
   Each request takes the workspace row lock (`workspace_tx`), so two requests never
   both fit under the same total.
4. The strictest match becomes the **decision record** on the action
   (`relay_control_actions.guard`): rule, revision, effect, summary, the facts used
   (the field's value; or the amount, today before, today after and the limit),
   whether the facts were known, and how many rules matched.

The record is part of every action view, so the approval dialog shows "Held by a
company rule" or "Blocked by a company rule" with the rule and its facts. Agents see
each action's rules in plain words with their operations (`company_rules`), so they
know a limit before they try.

A held request follows the ordinary approval path: it goes to the agent's managers,
and approving it sends the exact fields once. This works even when the account's
rule for that action is Allow: approval authority accepts an Allow rule for a request
the Guard held (`approval_delivery::decision_authority`). Changing a rule does not
re-check requests already waiting.

## Who can do what

- **Everyone** reads company rules: Settings → Company rules (`GET
  /api/identity/workspaces/{w}/guard/rules` returns rules with their summaries, the
  actions they can apply to with their fields, and `can_manage`).
- **Admins** add, change and turn rules off (`POST` the same path with
  `{id, operation, connection, kind, field, comparator, value, effect, note, enabled,
  revision}`). The server checks the action exists, the field exists, numbers are
  compared with number fields, totals use number fields with over/reach, and the
  revision matches. Up to 200 rules per workspace. Every save is in the Audit log in
  plain words.

## Schema 64

- `relay_guard_rules`: one row per rule, with database checks for the same kinds,
  comparisons, effects and value types.
- `relay_control_actions.guard`: the decision record, null when no rule applied.

## Checks

- PostgreSQL (`postgres/guard_tests.rs`): a field rule holds a large refund and
  records its facts in the approval and can be approved although the account allows the action; block wins over ask with two matches; a missing
  amount asks a person; rules never loosen an agent's Ask first; ten parallel refunds
  of 300 against a 1000 daily total leave exactly three waiting and seven blocked,
  and rejecting one frees its reservation; only admins save; stale revisions,
  unknown fields, number comparisons on text and text totals are refused; saves are
  audited.
- Real Goose (`tests/guard_rules.mjs`): an admin adds two rules in Settings with the
  read-back; a member sees them read-only and can't save one; saves are audited; on an
  Allowed action the agent's normal write runs, the blocked one is denied and the held
  one waits with nothing sent; the approval dialog says "Held by a company rule" with
  the fact used, and the agent's manager approves the exact write once. Screenshots at
  desktop, 390 px and dark; no horizontal overflow.
- Web (`tests/company-rules.test.mjs`): the read-back, one save per click burst, the
  draft kept after a failed save, the same rule identity on retry, text fields
  limited to is / is not, totals limited to number fields, turning off with the
  current revision, read-only for members, and the approval note with its facts.

## Not yet (next CA slices)

- Looked-up facts ("the order's total in Shopify" when the request carries only an
  order ID) and action meanings beyond read and write (money, sends outside, delete).
- Rules on outgoing messages, data reads and browser steps.
- Creating a rule from one sentence, replay on past activity and a trial week (CA2).
- A Rules page outside Settings and rules from managers for their own agents.
