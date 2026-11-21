# Forward: review and a smaller window (2026-11-21)

**Status: proposal. No application code changed.** The owner asked whether Forward
is built well, how its window could be smaller and better designed, and how to
forward a message with their own words and a document. The built behaviour is in
[`runtime/forwarding.md`](../../runtime/forwarding.md); the design approved on
2026-11-16 is [`designs/forward-and-invite`](../forward-and-invite/README.md).

`review.html` is the review with a working prototype of the proposed window beside
screenshots of the built one at the same scale. It is plain HTML with no build step;
it is written as an artifact page, so it has no `<html>` wrapper of its own.

## Found in the built window

Checked in the real client (Vite dev server, fixture API answers, 1280 × 800,
1280 × 600 and 390 × 844), screenshots in `built/`.

| | Finding | Evidence |
| --- | --- | --- |
| Bug | Focus lands on Close, not the search box. Typing does nothing; Enter closes the window. | `autoFocus` runs before the dialog's `showModal()`, which then focuses the first control. `Dialog` already has `initialFocus` for this. The jsdom tests stub `showModal`, so they pass. |
| Bug | Files are dropped silently. A files-only message forwards as the words "Shared files". | `message_forwarding.rs` copies `content` only and sends `attachments: vec![]`. `built/files-only-message.jpg`, `built/message-with-files.jpg`. Read in the server code, not run against PostgreSQL. |
| Design | 520 × 636 on open, 696 with an agent chosen, 768 (window height) and a scrolling body for a long message; 723 of 844 on a phone. | `built/opened.jpg`, `built/short-message.jpg`, `built/long-message.jpg`. |
| Design | The list is 190 px (two and a half rows); with three agents no person is visible. A choice that search filters out stays chosen with nothing highlighted. | |
| Design | No arrow keys; Enter in the search box chooses nothing. | |
| Design | "Owners and admins can see what you forward" is an amber box on the most common path. | |
| Small | A paused agent is not marked in the list. What the server does with a forward to one was not tested. | |

## Proposed window

440 wide; 480 to 530 tall while choosing, 330 to 390 once someone is chosen
(measured in the prototype).

- One **To** field. Recent people and agents first, one line each; typing searches
  everyone. The choice becomes a chip and the list closes.
- The message in three lines with "Show all"; its files listed, each removable.
- Your words in a box like Chat's, with the paperclip and **Forward to <name>** inside it.
  No footer; an error still shows in the dialog's footer line.
- One quiet line for who can see a shared agent's chat.
- Focus starts in To; ↑ ↓ Enter choose; focus moves to your message; Ctrl/⌘ + Enter sends.

## Documents

- **Files that came with the message.** `relay_objects` rows belong to one chat and
  only their uploader may attach them, so a forward cannot reuse the IDs. The server
  copies each file into the target chat inside the forward's transaction and names
  them in `forwarded`. Deleting the forward removes its copies.
- **Your own documents.** Uploads need a chat, and the direct chat may not exist yet.
  The window keeps the files until Forward, then finds or creates the chat, uploads
  and sends. The forward request gains `attachments`; its request ID must cover them.
- Eight files and 10 MiB each for the whole message, as today.
- To an agent: files from the forwarded message are information; your words and the
  documents you add are the request.

## Decisions for the owner

1. Do a message's files go along by default? Recommended: yes, each with an ×.
2. Who is listed first? Recommended: most recent conversation, people and agents
   mixed. This changes "agents first", approved on 2026-11-16.
3. How loud is "owners and admins can see"? Recommended: one quiet line.

## Suggested order

1. Fix focus; say when files are left behind and refuse a files-only message.
2. The smaller window (client only).
3. Carry the message's files (server, schema, client).
4. Attach your own documents.

Not covered: forwarding into channels or groups, to several people, or several
messages at once.
