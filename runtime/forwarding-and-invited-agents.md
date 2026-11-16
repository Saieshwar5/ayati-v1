# Forwarding messages and inviting agents into your own agent's chat (schema 125)

Built 2026-11-16. Two ways for work to move between people and agents without
new screens: **Forward** hands one message to a person or an agent, and a
**mention in your own agent's chat** asks your agent to work with another agent
in a thread you can read. Approved designs: the click-through shown on
2026-11-16 (`Forward and Mention in Cuckoo`).

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

## Mention another agent in your own agent's chat

In your direct chat with your own only-me agent, typing `@` also offers the
shared agents you may talk to, with the note "Works with <your agent> in a
thread". A message that mentions them:

1. asks **your own agent**, as every message in that chat does;
2. makes each mentioned agent a **guest of the thread under your message**
   (`relay_thread_guests`). Your agent answers in that thread;
3. your agent is told whom you invited and hands each of them its part with a
   hand-off, posted in the thread, where they answer. You don't have to do
   anything: read along, reply in the thread, or wait for your agent's answer.

The line under your message is the tracker: who is in the thread, the reply
count, who is working, **Needs you** while a question or approval waits for you,
and **Done** when every agent there has finished. In the thread, a note says
"HR Desk joined this thread only. It can't see the rest of this chat. Its owners
and admins can see the work done here." Plain replies there go to your agent.

Rules, enforced by the server:

- **A guest reads its thread and nothing else of the chat.** Every read of a
  conversation for an agent passes `thread_guests::confine`: a guest's prompt,
  its execution-time evidence and any other read are refused beyond its thread.
  It keeps no summary or brief of the chat, and the older-history tools are
  unavailable to it.
- **A guest is never a member.** It works there only on a hand-off from your
  agent (or another guest of the same thread), within the hand-off limits. It
  can't hand work back to your agent or to agents not invited.
- **Who can be invited:** shared, managed agents you may talk to, that take work
  from other agents, at most two per thread, only in your own only-me agent's
  chat. In any other direct chat or channel nothing changes: every mentioned
  agent must still be asked there.
- **Your agent shares only what each part needs.** Its guidance says not to
  pass on your private mail, files or other conversations unless you agreed to
  share that in the thread, and to ask you first.
- A second matter is a second message, so it gets its own thread; a guest of
  one thread can't be handed work from another.

## Schema 125

- `relay_chat_messages.forwarded JSONB`: the copy a forward carries.
- `relay_thread_guests(workspace, chat, thread, agent, invited_by, at)`, removed
  with its chat, thread or agent.

## Checks

- PostgreSQL `message_forwarding::tests`: the copy, the agent asked with your
  words, the evidence telling it the copy is information, one send per ID, the
  direct chat found again, deletion removing the copy, someone who can't read it,
  someone else's only-me agent, a bare forward to a person, forwarding it on,
  deleted and restricted messages refused.
- PostgreSQL `thread_guests::tests`: the thread under the message, the guest row
  and no membership, the invited guidance, the hand-off into the thread, the
  guest's stored prompt, execution evidence and prepared task free of earlier
  private messages, a wider read refused, no brief, no hand-off back, the answer
  saved; refused invitations (someone else's only-me agent, no work from agents,
  three agents, a third guest later), a retried message, a second matter that
  can't reach the guest, and no change in other direct chats.
- Unit: the invited guidance text.
- Web `tests/chat-forward.test.mjs` (the window, required words, one request on a
  double click, a failed forward kept, the copy shown), `tests/chat-invited-agents.test.mjs`
  (asking your own agent, the limit of two, Needs you and Done) and
  `tests/navigation.test.mjs` (the mention menu, your agent asked, the guest note,
  plain thread replies to your agent).

## Not built yet

Forwarding files and several messages at once, forwarding into channels, a guest
answering a person's question asked directly in the thread, a personal agent that
reads a mailbox (step 4 of the plan), and asking before your agent shares more of
your private information as a server rule rather than guidance (step 3).
