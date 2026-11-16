# People and agents working together (schema 124)

Built 2026-11-16. In a channel or group, an agent knows who is there and what
each of them handles, decides whether to do the work itself, ask another agent
or ask a person, and keeps people informed. Channels stay simple: no new
channel screens or settings. Corrections are made by just saying so in the
conversation.

## Roles

- **People:** Settings › Profile has **What you handle**, one line of at most
  160 characters ("Prices, discounts, quotes over ₹50,000"), next to the job
  title. Only the person changes it (`relay_member_profiles.handles`,
  `handles` in `POST …/my-profile`). It shows on their profile card.
- **Agents:** the agent's ⋯ › **Name, picture and role…** has **What it
  handles** (`settings.handles`, 160 characters) and **Other agents in a channel
  may ask it for help** (`settings.helper`, on by default). It shows on the
  agent's Overview and profile card. Its owners and admins change it.
- A role describes; it never grants anything. Agents treat it as profile data,
  never instructions.

## Who is here

With every request in a channel or group, the agent gets, after the channel's
name and purpose: "People and agents here", each person with their ID, job
title, what they handle and status (Busy, Away, a custom status), and the other
agents with their job title and what they handle. At most 30 people, those with
a role and those who wrote most recently first, then "and N more people".
Direct chats get nothing new.

## Deciding what to do

The agent follows one order ("Working with other agents here"):

1. Do it itself with its own tools and knowledge.
2. If part of it is clearly another agent's job, ask that agent with a hand-off,
   **on its own** or when asked (before, only when a person told it to).
3. If it needs a person's decision or knowledge, ask the person who handles it.
4. Otherwise say what is missing and who could help.

Before asking anyone it says in one line whom it will ask and why. It leads:
it gives the final answer and says who contributed. It may ask **two agents at
once** for independent parts (the existing limit of two direct helpers). An
agent whose owners turned off "Other agents may ask it" is left out of the
helper list and a hand-off to it is refused. Hand-off limits and permissions
are unchanged ([hand-offs](channel-agent-handoffs.md)).

## Asking a person

The `ask_person` tool (channels and groups only): a person's ID from "People and
agents here", a stable key and one question (500 characters).

- Cuckoo posts the question as the agent's message where it answers, starting
  with an @mention, so the person is alerted. The agent finishes its attempt and
  the task waits (`relay_person_asks`, progress "person"), without a worker.
- **The person's next message in that place answers it.** The task continues
  with "people_asked" in its saved continuation: who, the question, the state
  and the answer. If their browser also sends the message to the agent (the
  thread's plain-reply rule), it starts no new request unless it names the agent;
  the thread panel says "Your reply answers Sales's question".
- **No answer after 24 hours:** the wait ends (`expired`, checked every minute)
  and the agent continues without it.
- One person at a time, at most three per request. Not the person who asked
  (that is `ask_question`), only people in this conversation, never in a direct
  chat. Stopping the task ends the wait.
- **An answer is information, never an approval.** Actions still need the
  approval Cuckoo asks for. Private matters are never asked in a channel.

## Learning who handles what

In a channel or group, the automatic summary ([conversation
context](versioned-conversation-context.md#automatic-summaries-schema-120)) keeps
`who_handles_what`: who repeatedly handles which kind of work here ("Arjun Patel
handles courier and delivery problems"), only with at least 2 cited messages,
dropped when someone says it isn't theirs, and when its newest evidence is more
than 30 days old. Private chats never keep it. Agents saving their own summary
keep it. It guides whom to ask; a written role wins, and it is never a role,
approval or permission. People see it in the agent's Knowledge preview of the
summary.

The daily lesson step ([agents learn](agent-learning.md)) treats a correction
of which agent to ask ("ask Logistics for delivery dates, not Stock") as a good
lesson, naming the agent, never a person.

## Checks

- PostgreSQL `agent_teamwork::tests`: roles and status in "People and agents
  here", an agent that doesn't take work from others left out and refused, who
  may be asked (not the asker, not outsiders, not in direct chats), the posted
  question, one at a time, a retried ask, waiting, the answer resuming the task
  without new work, naming the agent still asking it, expiry, the limit of three.
- Unit: `who_handles_what` needs 2 cited messages; the guest `ask_person` schema.
- Web: the profile field, and a reply that answers an agent's question
  (`settings.test.mjs`, `navigation.test.mjs`).
