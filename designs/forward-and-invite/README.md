# Forward and invited agents (schema 125)

Approved 2026-11-16 from the click-through "Forward and Mention in Cuckoo" (real
client screens with sample data). These screenshots are the built feature,
taken on the scripted company simulation (`ayati simulate`) on 2026-11-16.

1. `1-forward-window.jpg`: Forward from a message's toolbar: agents first, your words, the copy, who else can see it.
2. `2-forward-landed.jpg`: the forward in your chat with HR Desk, and its answer.
3. `3-mention-menu.jpg`: in your own agent's chat, `@` offers other agents: "Works with My assistant in a thread".
4. `4-mention-thread.jpg`: your agent answers in a thread under your message; plain replies go to it.

The simulation's scripted agents answer without hand-offs, so the invited agent's
own replies, the "joined this thread only" note and the Done state are covered by
tests, not shown here. Contract: [runtime/forwarding-and-invited-agents.md](../../runtime/forwarding-and-invited-agents.md).
