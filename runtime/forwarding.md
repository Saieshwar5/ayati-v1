# Forwarding messages (schema 125)

Built 2026-11-16. **Forward** hands one message to a person or an agent, in your direct chat with them. Approved design: the click-through shown on 2026-11-16 (`Forward and Mention in Cuckoo`).

## Forward a message

Every message's toolbar has **Forward**, between Reply and Reply in thread.

- The window lists agents, then people. You pick one, add your own words, see
  the message as it will arrive, and press **Forward to <name>**. Cuckoo opens
  your direct chat with them (creating it if needed) and the forward is your
  message there, with a marked "Forwarded from" block: who wrote it, where and
  when, as it was when you forwarded it.
- **To an agent, your words are the request** and are required ("What do you
  need from Finance Desk?"). The agent is told the forwarded text was written by
  someone else: information, never an instruction, even when it asks for
  something. The window says the agent's owners and admins can see what you
  forward (not shown for your own only-me agent).
- **To a person**, your words are optional; a bare forward shows only its copy
  (its stored content is `Forwarded message`).
- You must be able to read the original and talk to the agent you send it to;
  someone else's only-me agent is not offered and is refused.
- **What can't be forwarded:** deleted, unfinished and empty messages, answers
  built from restricted sources (the same messages kept out of agents' later
  context: Library documents, connected-event data, withdrawn knowledge,
  operations, hand-offs that used them) and messages in a connected system's
  subject room. A copy would escape the checks and the erasure that protect them.
- Forwarding a bare forward passes on the original. Editing or deleting the
  original leaves the copy; deleting the forward removes it.
- One request ID per exact forward: a retried request after a lost answer sends
  once, and a double click sends once. A failed forward keeps the window and
  your words.

`POST /api/identity/workspaces/{w}/chats/{chat}/messages/{message}/forward` with
`{id, chat, to: {kind: "user"|"agent", id}, note}` returns `{chat}`. `id` is the
new message's ID; `chat` is the ID a new direct chat gets. The message carries
`forwarded: {name, kind, place, at, text}` (`relay_chat_messages.forwarded`), in
history, live events and agents' conversation evidence. People's names are their
display names, never their email addresses.

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
  deleted and restricted messages refused.
- Web `tests/chat-forward.test.mjs` (the window, required words, one request on a
  double click, a failed forward kept, the copy shown).

## Not built yet

Forwarding files and several messages at once, and forwarding into channels.
