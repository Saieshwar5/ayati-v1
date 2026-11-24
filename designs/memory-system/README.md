# The memory system (proposal, 2026-11-24)

**Status: built 2026-11-24**, all six steps, as described in
[`runtime/memory.md`](../../runtime/memory.md). Open [`mockups.html`](mockups.html)
for the pictures and screens, and [`implementation-plan.md`](implementation-plan.md)
for the build steps.

What differs from this proposal:

- **Strength starts at 50** for every agent note (the tools can't tell a person's
  words from the agent's own guess) and 90 for a person's. Use raises it only when a
  search finds the note.
- **Room notes in a channel** are seen by the agent's owners and admins who can open
  the room, on the agent's page; people in the room don't get a view of them yet.
- **Owners and admins get no page of person shelves**: those notes show only as
  lines on requests they can already open in Activity. The person sees and removes
  their own under About you.
- **No "Remembered · Undo" line in chat** and no "Memory here" in the context
  preview: the line is in Activity, and About you is where a person removes it.
- **Search by meaning, contradiction checks, putting a corrected answer's notes on
  hold and widening a direct-chat search to the person's channels** are not built. The workspace level was drawn first in
[`../workspace-memory/`](../workspace-memory/README.md); where the two differ, this
folder is newer.

## The idea in one paragraph

Agents learn while working. What they learn is kept at the level where it is
useful and safe: for the whole workspace, for one agent's job, for one channel,
or for one person. All levels use one engine (short notes, topic summaries,
strength, fading, a nightly tidy-up). The rooms Cuckoo already has decide which
levels an agent may read and write in each request. Beside memory, an agent can
search the real old messages of the conversation it is in.

## The five things an agent can know

| | Belongs to | Holds | Today |
| --- | --- | --- | --- |
| **Workspace memory** | the workspace | facts about the business any agent needs | new |
| **Agent memory: job shelf** | one shared agent | what it learned about doing its own job | new |
| **Agent memory: room shelf** | one agent in one channel or group | what was settled and learned there | new |
| **Agent memory: person shelf** | one agent with one person | how to work with that person | new |
| **Conversation history** | one conversation | the real messages, searched on demand | exists, weak search |

Unchanged and not part of this engine at first: a personal assistant's memory of
its person, an agent's notes and approved lessons (rules for how to work, written
or approved by people), the room brief (what is going on in a room right now),
skills and the Library.

## What each level remembers

**Workspace memory: what is true about the business, whoever asks.**
- Customers and suppliers: terms, contacts, preferences.
- Systems: where data lives, what breaks, when things lock.
- People and roles: who handles and who approves what.
- Decisions and policies: what was decided, and when.
- Words: what "the Friday report" or "PO-lite" means here.
- Never: private matters, secrets, live numbers (stock, balances, order status
  are read from the app each time), rules for agents, step-by-step how-tos.

**Job shelf: what this agent learned about its own work.**
- How its tools behave ("the sheet update fails over 500 rows; send batches").
- Where its things are ("quote templates are in the Sales folder").
- Its standing duties ("I post the stock report in #ops every Monday").
- What worked ("reminders sent before 11 am get answered the same day").
- Never: anything about one person; rules (those are lessons, approved by a person).

**Room shelf: what this channel has settled.**
- Decisions made here, the room's projects and their state.
- What people here usually ask for, and the format they like.
- Who handles what in this room.

**Person shelf: how to work with this person.**
- What they usually ask and how they like answers (short, in Hindi, with a table).
- Their open matters with this agent.
- Facts they told the agent about their work ("I handle the Pune accounts").
- Never: more of their private life than the work needs. They can read and remove it.

## Where a new fact is filed

The agent keeps a note with one tool and says who it is for. The server enforces
the limit; the agent's choice can only make the audience smaller, never wider.

| The agent chooses | It asks itself | The server allows it directly when |
| --- | --- | --- |
| **here** (default) | "Is this about this person or this room?" | always: it is filed on the shelf of the conversation the request runs in |
| **my job** | "Will I need this in every conversation?" | it was learned in a public channel or from the agent's own tool result; otherwise an owner approves it |
| **workspace** | "Would another agent need this?" | it was learned where the whole workspace may see it; otherwise an admin approves it |

Anything read from email, the web or a connected app waits for a person before it
can sit above "here". A background run (schedule, event trigger) can only suggest.

## What an agent may read in each place

One rule: **a memory is used only where everyone who reads the answer could have
seen where it was learned.** The server works this out from the running request.

| The request runs in | Workspace | Job shelf | Room shelf | Person shelf | History |
| --- | --- | --- | --- | --- | --- |
| Meera's direct chat | yes | yes | no | Meera's | this chat |
| #sales | yes | yes | #sales | nobody's | #sales |
| A thread with an outside customer | no | no | that thread's | no | that thread |
| A schedule or trigger's work room | yes | yes | that room's | no | that room |
| A person's own assistant | yes, read only | no | no | its own memory of its person | its chat |

A channel question that needs private memory moves to the asker's direct chat
with the existing `continue_privately`. Outside-customer threads get no workspace
or job memory until notes can be marked safe for customers, as the Library does.

## How an agent finds a memory

Most of the time it does not have to decide.

1. **Given with every request, no search needed** (fixed size, about 8,000
   characters in total): the workspace essentials, the agent's job essentials,
   the top notes of this room or person, and up to three topic summaries that
   match the request.
2. **One search, every allowed shelf.** `search_memory {query}` looks through
   all shelves this request may read and returns the best lines, each labelled
   with where it comes from: `[workspace]`, `[my job]`, `[this conversation]`.
   The agent may narrow it, but never has to pick the right shelf.
3. **A short ladder in its instructions** for what memory cannot answer:

| The agent needs | It looks in |
| --- | --- |
| a lasting fact (who, how, what was decided) | what it was given, then `search_memory` |
| what was said, when, by whom, exact words | `search_conversation_history` |
| the text of a document or policy | the Library |
| a live number (stock, balance, order status) | the connected app, never memory |
| nothing found | it says so and asks; it never guesses |

When notes disagree, the closer one wins: person or room, then job, then
workspace, unless an admin pinned the wider note. When memory and a newer message
disagree, the message wins and the agent ends the note.

## How we know it is searching

Every memory action is recorded by the server against the request, the way a
skill load is today.

- **Activity › a request** shows: "Given from memory: 9 notes", "Searched memory:
  'Acme payment terms' · 3 found", "Searched this conversation: 'pricing last
  month' · 5 found", "Remembered for Meera: …", "Ended a note: …".
- **In chat**, the line "Remembered: … · Undo" sits under a reply whenever the
  agent kept something about that person or room. The person sees it at once.
- **Looked for, not found** (Settings › Memory and an agent's Memory panel) lists
  what agents searched for and did not find. It tells people what to teach.
- **Numbers per agent and for the workspace:** searches a week, share that found
  nothing, notes kept, notes ended after a correction.

"Given", "searched" and "found" are exact, because the server does them. Which
note the agent really relied on is its own report and can be incomplete.

## Who can open what

| Memory | Reads | Changes |
| --- | --- | --- |
| Workspace | admins | admins |
| Job shelf | the agent's owners and admins | the same |
| Room shelf | people in the room (in Conversation context), owners and admins who can open the room | owners and admins who can open the room |
| Person shelf | the person; owners and admins as Activity already lets them open that person's requests, with the view audited | the person removes; owners and admins remove |

Correction to an earlier explanation: owners and admins can already open a
person's requests to a shared agent in Activity, with an audit line. The person
shelf follows that same rule instead of inventing a stricter one that Activity
would contradict.

## Screens that change

| Screen | Change |
| --- | --- |
| Settings › Memory | new admin page: workspace memory, Needs your answer, Looked for not found, a filter per agent |
| Agent page › Knows | two new things: **Memory** (job shelf, owners and admins) and **About you** (everyone who talks to it; today only an assistant has it) |
| Agent page › Knows › Lessons | also lists "Suggested for its memory": notes the agent wants to file wider |
| Conversation context preview | a "Memory here" section: what the agent would be given in this conversation |
| Chat | "Remembered: … · Undo" under a reply; no new screen |
| Activity › a request | the memory and history lines above |
| Settings › People › Remove | the hand-over summary says that person's shelves are deleted |

No new rail item, no new page outside Settings and the agent page.

## How sophisticated, and when

Strict from the first day: the room boundaries, the server choosing every scope,
and the record of every memory action. A leak or an invisible memory cannot be
repaired later.

Simple at first, smarter with data: word search before meaning search; notes
before topic summaries; a fixed strength before automatic fading and merging.
The tidy-up needs a few hundred real notes before its numbers can be set well.

## Risks this design accepts

- A model can still state a wrong fact as a note. Sources, strength, the Undo
  line and corrections ending notes limit the damage; they do not remove it.
- Refusing to widen private notes loses some learning.
- More text in every request costs tokens on every request; the fixed 8,000
  characters is the price, and it must be measured against answer quality.
- Removing a note does not remove what was already said in messages or backups.
