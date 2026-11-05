# Company rules: the Guard (schemas 64–67)

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

## What actions do

Every reviewed action has a meaning (schema 65): **read**, **change**, **money**,
**sends outside** or **delete**. The provider-facing effect (read/write) stays as it
was; the meaning is Cuckoo's own review.

- Cuckoo suggests one from the action's id, tool and title (`relay_guess_meaning`):
  refunds, payments, invoices and transfers are money; send, email, message, reply and
  post are sends outside; delete, remove and archive are deletes; other writes are
  changes. Settings → Connections shows it as "(suggested)" until an admin reviews it.
- Admins confirm or correct it per action (`POST /api/identity/workspaces/{w}/control/action-meanings`
  with `{connection, operation, meaning, revision}`). Reading actions are always read;
  writing actions can't be read. Waiting approvals for that action ask again, and the
  change is in the Audit log ("Reviewed Update an order as money action").
- **Safe defaults** follow the meaning when an admin set no rule (`relay_default_mode`):
  reading Allow, deleting **Block**, everything else Ask first. Before schema 65 deletes
  asked first.
- Each request remembers its meaning (`relay_control_actions.meaning`).

## Rules for every action of a kind, and time rules

- A rule can name **one action** (and optionally one account) or **every action of one
  meaning**: "Any money action: if today's total amount would go over 5000, block it."
  A kind-wide rule covers only actions that have its field; a kind-wide daily total adds
  up every request of that meaning today, across apps.
- **Time of day**: `kind: "time"` with `value: {"from":"21:00","to":"09:00","days":[6,7]}`
  (days 1 = Monday … 7 = Sunday; none for every day; equal times mean all day; windows
  may wrap midnight). It uses the workspace's time zone, and records the local time and
  day as its facts: "Any action that sends outside: on Saturday and Sunday, between 21:00
  and 09:00, ask first."

## Facts the agent didn't send, and "read first"

Many actions carry only an ID: a refund names the order, not its total. A rule can take
its value from a **reading action the agent completed earlier in the same task**
(schema 66), using Cuckoo's own receipt of that read, not anything the agent supplies:

- `source: {"operation":"order.read","match":"order","path":"total"}` uses the latest
  completed `order.read` on the same account in this task whose `order` equals the
  request's `order`, and reads `total` from its result: the provider's structured
  content, the result itself, or JSON text in its first content item (dotted paths;
  numbers written as text count). "Refund an order: if total from Find an order is over
  2000, ask first."
- No such read, or no such value: **a person decides**. The decision record says where
  the fact came from (`facts.from`: the read and when).
- `kind: "read_first"` needs that read first: "Update an order: unless Find an order for
  the same order ran first in this task, block it." Its facts say whether the read was
  found.
- The Guard never calls an app while deciding, so no request waits on another app while
  the workspace is locked. The reading action must be on the same account and share the
  matched field; the editor offers only such reads.

## Creating rules safely: proposals, replay and trials

Schema 67, core-agent CA2.

- **Agents propose, admins decide.** When a person asks an agent for a rule ("refunds
  over 2000 need my approval"), the agent calls `propose_rule` with their words and the
  structured rule. The controller checks it against the actions **that agent uses** and
  saves it **off and unreviewed** (`reviewed=false`, `proposed_by`, `proposal`). Up to 20
  wait at a time. It's in the Audit log. An agent can't turn a rule on, edit one or
  loosen one. Invalid drafts return `{"state":"invalid"}` with the reason.
- Settings → Company rules shows **Proposed by agents** with the person's words, the
  code-made read-back and **Review** / **Dismiss**. Reviewing opens the editor; **Turn on**
  saves it (`reviewed=true`, enabled). Dismiss deletes a proposal nobody turned on
  (`POST …/guard/rules/{id}/dismiss`), audited with the person's words.
- **Replay** (`POST …/guard/rules/replay`, admins): what the draft would have done to
  the last 30 days of requests (up to 5,000, excluding invalidated ones): how many
  matched, would have been held and blocked, and up to 10 examples. It uses the
  workspace's days and times, earlier reads in the same task, and the day's totals of
  what really happened. It saves nothing.
- **Trial week**: saving with `trial: true` sets `trial_until_ms` a week ahead. Until then
  the rule only records what it would have done (`guard.trial` on the action, counted as
  `trial_matches`) and changes nothing; then it enforces. Turning a rule off clears its
  trial.

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

## Schemas 64–67

- `relay_guard_rules`: one row per rule, with database checks for the same kinds,
  comparisons, effects and value types.
- `relay_control_actions.guard`: the decision record, null when no rule applied.
- Schema 67: `relay_guard_rules.proposed_by`, `proposal`, `reviewed`, `trial_until_ms`.
- Schema 66: `relay_guard_rules.source` and the `read_first` kind.
- Schema 65: `relay_action_meanings`, `relay_guess_meaning`, `relay_action_meaning`,
  `relay_default_mode`, the effective-rules view using it, `relay_control_actions.meaning`,
  and `relay_guard_rules.meaning` with time rules (constraints renamed and widened).

## Checks

- PostgreSQL (`postgres/guard_proposal_tests.rs`): a proposal stays off and isn't in
  force; invalid drafts are refused; members can't dismiss; the admin turns it on and it
  holds; reviewed rules can't be dismissed; dismissals are audited; a trial records
  without blocking, counts its matches and then enforces; replay counts holds, blocks and
  daily totals on past requests, members can't run it and it saves nothing.
- Real Goose (`tests/guard_rules.mjs`, last part): the agent proposes a rule from the
  person's words; the admin reviews, replays and turns it on in the browser; it then
  holds the matching change.
- PostgreSQL (`postgres/guard_fact_tests.rs`): with no read a person decides; a small
  read total runs, a large one waits with where the fact came from; read-first blocks a
  blind change and a read of another order, and allows after reading the same order;
  sources that aren't a read on the same account, don't share the field, or mismatch
  their path are refused.
- Real Goose (`tests/guard_rules.mjs`, last part): a change without reading the order
  first is blocked; reading it and changing it in one task runs.
- PostgreSQL (`postgres/guard_meaning_tests.rs`): meanings are suggested, a delete is
  blocked by default, admins review meanings (members can't, reading stays reading,
  stale revisions are refused, audited); one money rule covers refunds and payments and
  their day's total adds up across both; a time rule holds a request in its window in the
  workspace's time zone, not on another day, and bad windows are refused.
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

- Checking a change after it ran (reading the record back).
- Rules on outgoing messages, data reads and browser steps.
- A Rules page outside Settings and rules from managers for their own agents.
