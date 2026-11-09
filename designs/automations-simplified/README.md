# Automations, simplified (approved 2026-11-09)

Approved by the owner on 2026-11-09. It amends the Agents page (`designs/agents-simplified/`,
D233) and Activity (`designs/activity/`, D235) so that everything an agent does on its own has
one home and one set of words. The example company, people and numbers are fictional.
`prototype.html` is the clickable source; open it in a browser.

**Rule:** every thing is created and edited in one place, its home. Other places may show it
and offer quick actions that can be undone (Pause / Resume), always with a link to its home.

| Place | Job | Home of |
| --- | --- | --- |
| Chat | Ask agents for work | Conversations |
| Activity | Watch and answer: needs me, now, upcoming, history, review | Answers to approvals and questions |
| Agents | Set an agent up, including what it does on its own | Instructions, Tools rules, Automations, Model |
| Apps | Apps and connections | Apps and their data |
| Settings | Rules for everyone | People, allowed models, workspace rules |

## One word per idea

| Use | Means | Replaces |
| --- | --- | --- |
| Automation | Anything an agent does on its own | "Schedules and triggers", "routines" |
| Schedule | Runs at a time, once or repeating | "Routine" |
| Trigger | Runs when something happens in an app | "Event trigger", "App events" |
| Follow-up | The agent checks back on its own task | Task status "Scheduled" → "Follow-up set" |
| Upcoming | Everything that will run, workspace-wide | "Coming up" (Now), Tasks › Scheduled |
| Review | How agents are doing, with suggestions | "Progress" |

| File | Screen |
| --- | --- |
| `0-map.png` | Where things live, and the words |
| `1-automations-tab.png` | Agent › Automations: On a schedule, When something happens; app triggers listed too |
| `2-new-automation.png`, `2b-new-automation-review.png` | One "New automation": At a time or When something happens, details, read-back |
| `3-upcoming.png` | Activity › Upcoming: everything that will run, Pause / Resume in place, Edit links |
| `4-ask-in-chat.png`, `4b-member-automations.png` | Schedule card in Chat; members are pointed to Chat instead of a dead end |
| `5-review.png` | Activity › Review: suggestions open the agent's Tools instead of changing rules |

## Built in five pull requests

1. Automations tab and the words above; app triggers listed on the agent.
2. Ask in Chat: the schedule card and the members' empty state.
3. One home per thing: Review, suggestions that open the agent, the agent Activity banner.
4. New automation: one button and a three-step flow.
5. Activity › Upcoming.

Server rules don't change: resuming still needs the person who set a schedule up (or a fresh
review of access for triggers), and an app's builder or an admin changes its triggers in the app.

## Notes from building (2026-11-09)

- **Members' own schedules.** The owner approved letting a member save the schedule an
  agent proposes to them in Chat. It runs as them, with their access. A schedule that
  posts into a conversation stays private to the people in it, so managers see a count
  ("N more … stay private") instead of the row; pausing the agent still stops it.
- **Resuming a trigger** needs its access reviewed again, so Upcoming pauses triggers in
  place but resumes them on the agent ("Review and resume in <agent>").
- **Tasks** keeps its "Follow-up set" group: those are open tasks. Upcoming lists the
  follow-up itself; "Coming up" on Now is replaced by a link to Upcoming.
- The Review table's weekly-review switch stays where it is for now.
