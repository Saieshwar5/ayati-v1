# Workspace memory (proposal, 2026-11-24)

**Status: proposal, not built.** Nothing here exists in code. Open
[`mockups.html`](mockups.html) for the screens and the diagrams.

The idea: agents save what they learn about the business while working, and every
agent uses it. It is one memory for the workspace, next to the three that exist:
an assistant's memory of its person ([assistants](../../runtime/assistants.md#memory-schema-138)),
an agent's approved notes on how to work ([agent learning](../../runtime/agent-learning.md))
and the documents people write ([Library](../../runtime/library.md)). None of those
carries a fact one agent learned to another agent.

## Decided by the owner

- **Only admins see the memory and control it.** Members have no page and no route.
- **Agents update it themselves** while working.
- It must hold a very large number of memories and stay useful as it grows.

## Where it shows

- **Settings › Memory**, an admin page between Agent rules and Usage. No new rail
  item: the design system keeps all workspace administration in Settings.
- **Needs your answer** panel on top (the Apps pattern), then round filters with
  counts: Topics (default), Notes, Essentials, Fading, Archive, Changes.
- **A topic page:** its summary, then its notes with who learned each, where, when,
  how often it was used and a five-step strength meter with a word. The ⋯ menu is
  Pin, Confirm it is right, Make it an essential, Correct the wording, No longer
  true, Remove for good.
- **Activity › a request** gains "Used from memory: …" and "Remembered: …", shown to
  admins only, each opening its note.

## Three levels

| Level | Size | What it is | How agents get it |
| --- | --- | --- | --- |
| Essentials | about 30 lines | Pinned notes, then the strongest | With every request |
| Topics | hundreds | One summary per customer, system, process, team or project | The 3 that match the request best (word and meaning search, as the Library does) |
| Notes | tens of thousands | One short fact with agent, source, date, kind, strength | Only through a search tool |

Essentials and matched topics share a fixed budget (about 7,000 characters), so
the size of the memory does not change what a request costs.

## A note's life

New → Active (strength 30+) → Fading (10–29: searchable, ranked last, listed for
the admin) → Archived (under 10: agents no longer see it, an admin can restore) →
Deleted (after 12 months in the archive; the admin sets the time).

Three side states: **Pinned** (never fades, never summarised away, wins),
**Ended** (stopped being true; kept beside its replacement, unused) and
**In the summary** (its content lives in the topic summary; kept as evidence).

## Strength (0–100)

Starts at 40 when an agent noticed it once, 60 when a person said it in a shared
conversation, 90 when an admin wrote or confirmed it. A second agent learning the
same thing adds 15. Each use in an answer nobody corrected adds 3 and restarts the
fading clock. A person correcting an answer that used it puts it on hold and asks
an admin. Unused, a note loses half its strength per period, by kind:

| Kind | Half in |
| --- | --- |
| Status ("on leave this week") | 2 weeks, or its end date |
| People and roles | 3 months |
| Customers, Systems | 6 months |
| Decisions and ways of working | 12 months |

All numbers are starting values to tune on a real workspace.

## Nightly tidy-up, in order

1. End notes whose date has passed.
2. Merge notes that mean the same; strength and uses add up.
3. Settle disagreements: the newer, stronger note wins and the other is ended; a
   close call goes to Needs your answer.
4. Rewrite the summary of a topic with more than 25 notes or notes older than 90
   days; those notes move into the summary.
5. Fade unused notes and move them between Active, Fading and Archived.
6. Delete archived notes past the keep time.
7. Choose the essentials again.
8. Drop a summary sentence whose notes are all archived, unless pinned.

Every change is listed under Changes and can be undone by an admin.

## What an agent may save

- Only what it learned where the whole workspace may see it (public channels,
  Library items shared with the workspace). The server refuses the rest. Members
  can't open the memory, but agents repeat it in answers to everyone.
- Anything from email, the web or a connected app waits in Needs your answer.
- Facts, never instructions. Agents add and end; only an admin rewords or removes.
- No passwords, keys or card numbers.

## Open before building

- Whether notes from shared but not public places (a private channel, a folder
  shared with selected people) are refused, or saved with that audience and used
  only for it. The proposal refuses them, which is simpler and loses some learning.
- Whether a memory line an agent used is ever shown to the member who asked.
- The limits: notes per workspace, the essentials budget, the keep time.
- Whether the tidy-up needs its own model setting, as agent learning has.
