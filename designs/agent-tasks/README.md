# Agent tasks and calendar (approved 2026-11-24)

**Status: built 2026-11-24** except briefs (step 6) and alerts outside the app (step 8),
as the owner asked; see [`runtime/tasks.md`](../../runtime/tasks.md#since-schema-144-tasks-with-a-lifetime)
for what exists. What differs from the screens: the Calendar keeps a **List** view
(Planned's list, with Pause and Resume for schedules and triggers); there is no time zone
per agent yet; Cuckoo does not check end conditions in connected apps itself (the agent
checks them at each run); and there is no spending limit per task.

The owner approved this direction on 2026-11-24 and asked for every step to be
built except briefs (step 6) and alerts outside the app (step 8). The screens are
on the design canvas: <https://claude.ai/artifact/HFpRHe1SajWr3qrwGj3aKz>
(private to the owner until shared). Names, companies and numbers on it are
examples. The research behind it is in the owner's Drive (proj-docs,
`always-on-agents-research.md`), next to this plan (`agent-tasks-plan.md`).

## The rule

**People talk; the agent does. Screens are only for looking and stopping.**

- A person creates a task by saying it in chat: "Remind Sharma Traders about
  invoice 311 every three days until they pay." The agent creates it at once and
  answers "Done", with one line under its reply: the task, its next run, **View**
  and **Undo**. There is no form and no confirmation step.
- A person changes a task by saying it: "Make it every 2 days."
- An agent may create a task on its own when it sees work to do, and says so,
  with the same line and Undo.
- Anyone who can use an agent may give it a task. The task runs with only what
  that person and the agent may both use. Action rules (Allow, Ask first, Block)
  still apply to what the task does.

## One task with a lifetime

Schedules, event triggers, follow-ups and an assistant's regular checks become
one thing: a task. Every task answers the same questions.

| Question | Example |
| --- | --- |
| What to do | Remind Sharma Traders about invoice 311 |
| For whom | Meera |
| Wakes | Every 2 days at 10:00, India time |
| Ends | When invoice 311 is paid |
| Gives up | After 5 reminders, then tells Meera |

The shapes people need are all this one record:

| Shape | Example | Ends |
| --- | --- | --- |
| One run | Send the quotation to Acme on Friday at 15:00 | After the run |
| Never-ending duty | Every weekday at 9:00, check stock below reorder level | Only when stopped |
| Ends at a time | Daily sales post until 12 December | On a date or after N runs |
| Ends on an event | Chase invoice 311 until paid | When a condition is true, or at its give-up point |
| Long job | Prepare the quarterly report | When the work is done; the agent continues later between parts |

**The platform owns the repetition.** After each run Cuckoo books the next wake-up
from the rule. The agent never has to remember to come back.

## Decided by the platform, not by people

- **Time zone:** India time (`Asia/Kolkata`) unless the workspace changes it.
- **Missed run:** runs once when the system is back if it is still recent;
  otherwise it is skipped and recorded.
- **Last run still going:** the new one is skipped.
- **Nothing new:** the agent stays silent; the run is recorded as "no change".
- **Failures:** three in a row pause the task and put it in Needs you.
- **Spending:** every task has a built-in limit and pauses at it.

## Screens

1. **Chat**: tasks are made, changed and undone here.
2. **Agent page**: the "On its own" row lists every task of the agent.
3. **Activity › Calendar** replaces Planned: a week of runs, done, failed, planned
   and moved; tasks with no fixed time in a side list.
4. **Task page**: the rule as one sentence, the runs, the agent's notes, "Is it
   finished?", and Pause, Skip next, End task. Changes are made by asking in chat.
5. **Needs you** gains paused tasks, budget warnings and tasks that gave up.
6. **Settings › General** gains time zone, working days, working hours and holidays.

Briefs and the alerts page are drawn on the canvas but are not part of this build.

## Build steps

See [implementation-plan.md](implementation-plan.md).
