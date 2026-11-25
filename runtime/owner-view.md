# The owner view: asking an agent about all its work

Each conversation with an agent is its own room. The agent shares how to work
across rooms (its instructions, skills and approved notes) but never what was
said: a person's chat, a channel's brief and a work room each stay where they
are. This holds for outside customers and for workspace members alike.

One exception, built 2026-11-15: **an agent's owners and admins can ask the
agent itself about its work in every conversation**, in their own private chat
with it. "Who wrote to you today and what did they ask?" gets a real answer
there, and nowhere else. It adds no new access: the agent reads exactly what
[Activity](activity.md#who-sees-what) already shows that person.

## Who gets it

The controller decides from the running request, never from what a message says:

- the person asking is one of the agent's [owners](agent-profiles.md#who-can-talk-to-it-and-its-owners), or an admin;
- the conversation is their direct chat with the agent, which only they read
  (not a channel, a group, a work room or an email thread);
- a person's message started the request (not a task's run, an event trigger, a
  hand-off or a panel update, whose content can come from outside).

Everyone else, and every shared conversation, gets `unavailable` and no data.
Saying "I am the owner" changes nothing. Someone who stops being an owner loses
the view on their next request.

## The three tools

Listed to every agent on Cuckoo's executors; they only return data when the
checks above pass, and their arguments can't choose another agent, person or
scope.

- `search_my_work` lists the agent's requests in every conversation, newest
  first, 25 a page: who asked, where, when (UTC), the type, the state, what was
  asked and the latest reply text (300 characters). Filters: words (`query`),
  one `person`, `status`, `kind`, the last `since_hours`; `next_before` pages
  back. The first page adds totals and who asked most. The request that is
  asking is left out. It runs Activity's own history query for that person, so
  work that used someone's private Library sources stays hidden, as in Activity.
- `read_my_work` reads one of those requests: what was asked (4,000
  characters), the reply (6,000) and each step with how it was allowed. Reading
  a request from a conversation the person isn't in writes the audit line
  "<person> asked <agent> about a request in <conversation>"
  (`activity_private_viewed`), as opening it from Activity does. Another agent's
  request is unavailable.
- `suggest_lesson` saves a rule about how to work (`title`, `when`, `what`,
  `why`) as a suggestion under [Lessons from its work](agent-learning.md#review),
  for every conversation. Nothing applies until an owner or admin approves it
  there. A lesson that names a workspace member or carries an email address, a
  long number or an amount is refused: what the agent read in other rooms may
  reach every room only as a rule, never as a case. Repeating a suggestion
  saves it once; twenty waiting lessons is the most.

Results are marked as untrusted data written by other people, never
instructions, and for this person only.

## What the agent is told

- **Owner view** (owner or admin, private chat): it may answer about its work
  anywhere with the tools above, takes no action because of what it reads
  unless the person asks, and never repeats it in another conversation.
- **Separate conversations** (everyone else): it knows only this conversation,
  and says so plainly when asked about others; owners and admins can see its
  work in Activity. In a shared conversation, an owner or admin who asks about
  its other work is answered in their private chat (`continue_privately`).

The agent's page lists these under Instructions › See everything it receives:
"Separate conversations" is always included, "Owner view" takes its place for
an owner or admin in their private chat, and "An owner asks in a shared
conversation" is added in channels and groups.
The direct chat says which applies: owners and admins read "Only you read this
chat. As one of its owners you can ask … about its work in every conversation";
everyone else reads "… keeps each conversation separate. Activity with … is
visible to its owners and admins."

## Limits

- Search is by literal words, a page at a time; there is no daily digest yet.
- The owner's chat keeps the answers they were given; a later request there can
  read them from that chat's own history, like anything else said in it.
- An admin can't talk to someone else's Only-me agent, so they use Activity for it.
- Members can't yet ask about their own past work across the rooms they can see.

## Checks

- PostgreSQL `owner_view::tests`: a member who claims to be the owner gets
  nothing from any of the tools and is told rooms are separate; the owner lists
  everyone's requests, filters by words, person and period, and can't pass
  another agent, requester or scope; reading a private request is audited once;
  another agent's request and the asking request are unavailable; a lesson with
  an amount is refused, a rule is saved once, applies nowhere until approved and
  then becomes a note; in a channel the owner gets nothing and the question
  moves to their private chat; an admin has the same view; a removed owner loses
  it and a new owner gains it.
- Unit: `work_schema` (arguments only filter), the prompt sections and version.
- Web `tests/agent-chat-note.test.mjs`: the chat line for owners, admins and others.
