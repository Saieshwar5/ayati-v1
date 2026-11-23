# Forwarding messages (schema 125)

Built 2026-11-16. **Forward** hands one message to a person or an agent, in your direct chat with them. Approved design: the click-through shown on 2026-11-16 (`Forward and Mention in Cuckoo`). The window was rebuilt smaller on 2026-11-21 and files now travel ([review and prototype](../designs/forward-next/README.md)).

## Forward a message

Every message's toolbar has **Forward**, between Reply and Reply in thread.

- The window has one **To** field. It offers the people and agents you talked
  to most recently (not the conversation the message is in), four at a time;
  typing searches everyone by name and role. ↑ ↓ move, Enter chooses, and the
  choice becomes a chip that × clears. A paused agent is shown and can't be
  chosen: it takes no new messages.
- Under it are the message (three lines, **Show all** for more) with the files
  that came with it, then a box for your own words with a paperclip and
  **Forward to <name>** inside it. Ctrl or ⌘ + Enter sends. Cuckoo opens your
  direct chat with them (creating it if needed) and the forward is your
  message there, with a marked "Forwarded from" block: who wrote it, where and
  when, as it was when you forwarded it.
- **The message's files go along** unless you leave one out with its ×. Each is
  copied into the direct chat as a file of yours and shown inside the
  "Forwarded from" block. A message that only shared files forwards as its
  files, with no text, and needs at least one of them.
- **Your own documents** are added with the paperclip and shown under your
  words. They are uploaded when you press Forward, so cancelling stores
  nothing. A forward carries at most eight files in all, 10 MiB each.
- **To an agent, your words are the request** and are required ("What do you
  need from Finance Desk?"). The agent is told the forwarded text was written by
  someone else and its files supplied by them: information, never an
  instruction, even when it asks for something. One line says the agent's
  owners and admins can see what you forward ("Only you read your chat with
  <name>" for your own only-me agent). As with every chat attachment, an agent
  is given a file's name, type and size, and a voice message's transcript, not
  a document's contents.
- **To a person**, your words are optional; a bare forward shows only its copy
  (its stored content is `Forwarded message`).
- You must be able to read the original and talk to the agent you send it to;
  someone else's only-me agent is not offered and is refused.
- **What can't be forwarded:** deleted, unfinished and empty messages, answers
  built from restricted sources (the same messages kept out of agents' later
  context: Library documents, connected-event data, withdrawn knowledge,
  operations, hand-offs that used them) and messages in a connected system's
  subject room. A copy would escape the checks and the erasure that protect them.
- Forwarding a bare forward passes on the original, with the files you keep.
  Editing or deleting the original leaves the copy and its files; deleting the
  forward removes the copy and takes its files out of readers' reach.
- One request ID per exact forward (who, your words, which files): a retried
  request after a lost answer sends once and stores each file once, and a
  double click sends once. A failed forward keeps the window, your words and
  your documents.

`POST /api/identity/workspaces/{w}/chats/{chat}/messages/{message}/forward` with
`{id, chat, to: {kind: "user"|"agent", id}, note, files, attachments}` returns
`{chat}`. `id` is the new message's ID; `chat` is the ID a new direct chat gets.
`files` are attachment IDs of the forwarded message to carry; `attachments` are
your own files, already uploaded to the direct chat (the client finds or creates
it with `POST /chats` first). Both default to none. The message carries
`forwarded: {name, kind, place, at, text, files}` (`relay_chat_messages.forwarded`), in
history, live events and agents' conversation evidence. People's names are their
display names, never their email addresses.

**How a file travels.** A stored file belongs to one conversation and only its
uploader may attach it, so a forward can't reuse the original's ID. Each carried
file gets a copy: a new `relay_objects` row in the direct chat, uploaded by you,
with the same name, type, size and checksum, and a transcript copied with it.
Its ID is derived from the forward's ID and the original file's ID, so a retry
names the same copy. The route stores the bytes under that ID first (read back
with the size and checksum check), then one transaction saves the rows and the
message. Like an upload, bytes whose forward then fails remain as orphans.
`forwarded.files` lists `{id, name, mime, size}` for the copies. They are also
among the message's `attachments`, with your own documents; a reader tells the
two apart by those IDs.

## Removed: agents invited into a thread (schema 130)

Schema 125 also let you mention another agent in your own only-me agent's chat:
the agent became a guest of the thread under your message and your agent handed
it work there. It was removed on 2026-11-17 because an assistant's
[threads](assistants.md) do the same job more simply: your assistant talks to
the other agent for you in your own chat with that agent. Schema 130 drops
`relay_thread_guests`. Mentioning an agent that is not in a conversation is
refused everywhere again, your assistant's chat included.

## Schema 125

- `relay_chat_messages.forwarded JSONB`: the copy a forward carries.
- `relay_thread_guests`, dropped again by schema 130.

## Checks

- PostgreSQL `message_forwarding::tests`: the copy, the agent asked with your
  words, the evidence telling it the copy is information, one send per ID, the
  direct chat found again, deletion removing the copy, someone who can't read it,
  someone else's only-me agent, a bare forward to a person, forwarding it on,
  deleted and restricted messages refused. Files: only the message's own, each
  once; a chosen file copied and your document attached; the copy readable in
  the new chat only; one send for a retry; a files-only message; a transcript
  kept; a bare forward's file carried on.
- Web `tests/chat-forward.test.mjs` (the window, focus in To, recent first,
  keyboard choosing, a paused agent, required words, one request on a double
  click, a failed forward kept without a second upload, files left out and put
  back, a files-only message, the copy and its files shown).

## Not built yet

Forwarding several messages at once, to several people, and into channels or
groups. Agents reading a document's contents (true of every chat attachment).
