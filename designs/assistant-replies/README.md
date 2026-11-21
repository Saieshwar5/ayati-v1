# Assistant replies: one switch per chat (2026-11-21)

**Status: proposal. Nothing is built.** The owner found assistant threads
([`runtime/assistants.md`](../../runtime/assistants.md#threads-schema-128)) too
complicated to use and asked for a simpler way to let a personal assistant handle
messages from people and agents.

Open [`mockups.html`](mockups.html) in a browser. It holds a working example chat
(canned replies, no model), the two workflows as diagrams, and pictures of the other
places the feature shows.

## The proposal

One switch in a direct chat's header, with three positions:

- **Off.** The assistant does nothing there unless asked for a draft.
- **Ask me first.** Each reply is written into the message box. The person sends it.
- **Reply for me.** The assistant sends as the person, asks when it needs a decision,
  and steps down to Ask me first by itself when the matter is settled, after 30 sent
  messages, or on a problem.

It replaces the two ways to start a thread, the take-over window, and Stop, Resume and
Delete. Messages stay the person's own, with the Audit log.

## The @name tag and asking by mention (owner's suggestion, 2026-11-21)

- **The tag.** A message the assistant wrote shows its name as a mention on a line
  above the message, in place of the mark and name beside the time: "@Ayati sent this
  for you" when it went out on Reply for me, "Written with @Ayati" when the person
  pressed Send. Only its author sees the tag, as with today's mark.
- **Asking by mention.** Typing @ in any message box lists the person's own assistant
  first. A message that starts with @Ayati is never posted: the Send button reads
  "Ask Ayati" and the reply is written into the box. Words after the mention say what
  to write. The mark beside Attach inserts the mention.
- **One message.** The mark on a message's toolbar becomes "Reply with @Ayati": one
  reply to that message, quoted above it when sent. It no longer hands the chat over.

The example chat has a "You see / Meera sees" switch that shows the other side's view.

## Decisions still open

1. Who sees the tag: only its author (this proposal, as today), or also the other
   side on messages the assistant sent without the person.
2. After a matter is settled: step down to Ask me first (proposed) or to Off.
3. Cost: Ask me first runs the model once per incoming message where it is on.

## Checked

The page was rendered in Chromium at 1280 px (light) and 400 px (dark) with no page
overflow and no script errors. A scripted click-through of the example chat checked: a
waiting reply sent with its tag, the mention list, "Ask Ayati" posting nothing, a reply
to one chosen message with its quote, Reply for me sending with its tag, the question
bar, the step-down, and no tags or private lines in the other side's view. One clipped
picture found in that look was fixed afterwards and not rendered again. It follows
[`runtime/design-system.md`](../../runtime/design-system.md): the header pill, the
one-line bar inside the message box and the thin private line already exist; no banner
across the conversation and no row above the message box are added.
