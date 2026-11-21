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
Delete. Messages stay the person's own, with the private mark and the Audit log.

## Decisions still open

1. Sent as the person (kept in this proposal) or as the assistant.
2. After a matter is settled: step down to Ask me first (proposed) or to Off.
3. Removing "take over" from a message's toolbar (proposed).
4. Cost: Ask me first runs the model once per incoming message where it is on.

## Checked

The page was rendered once in Chromium at 1280 px (light) and 400 px (dark) with no
page overflow and no script errors, and the example chat was clicked through Send, a
change to Reply for me and two incoming messages. It follows
[`runtime/design-system.md`](../../runtime/design-system.md): the header pill, the
one-line bar inside the message box and the thin private line already exist; no banner
across the conversation and no row above the message box are added.
