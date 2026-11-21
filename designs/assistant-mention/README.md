# Asking your assistant with a mention (2026-11-21)

**Status: proposal. Nothing here is built.** The owner asked for designs and an
explanation of one idea: instead of the Draft button, a person mentions their own
assistant in any conversation and says what to do. The assistant writes a draft that
only its person sees; the person sends it as their own message, or discards it.

Open [`mockups.html`](mockups.html) for the screens. What is built today is in
[`runtime/assistants.md`](../../runtime/assistants.md) (drafts, schema 129).

## The design in short

- **Any name.** "Ayati" is only the name an assistant has until its person renames it.
  A mention holds the assistant's ID. The @ list shows a person only their own
  assistant, first, marked "Your assistant"; `@assistant` and `@my` find it too.
- **The message box changes** once the assistant is mentioned: purple border, the line
  "Asking <name> · Only you see this. Nothing is posted in #channel", and the button
  reads "Ask <name>". A team agent's mention stays public and unchanged.
- **The instruction is never posted.** The client intercepts the send, and the server
  refuses a posted message that mentions an assistant, so a broken mention can't leak.
- **A private draft card** above the message box: where it will be sent and as whom,
  the text, what the assistant used, then Send as me, Edit, Discard and "Ask <name> to
  change it". The box stays free while the assistant works.
- **Sent as the person.** Others see an ordinary message. The private mark, the Audit
  line and learning from edits stay as built.
- **"@<name> respond"** answers the newest message meant for the person; the card
  quotes what it answers.
- **A stale draft** (new messages since it was written) says so and offers Update.
- **No wording grants sending without review.** Take over stays its own action.

## Choices left to the owner

1. Card above the box (recommended) or keep writing the draft into the box, which
   replaced an earlier card on 2026-11-19.
2. "Respond" in a busy channel: the newest message that mentions the person
   (recommended), or pick a message first.
3. Keep the Draft button as a shortcut that inserts the mention (recommended).

## What building it would touch

The draft request already takes the conversation, thread and instruction
(`POST …/assistant/drafts`). New: the assistant in the mention list
(`web/src/react/chat/mentions.ts` treats every agent mention as a public ask today),
the message box mode, the card, a longer wait than two minutes for research requests,
and the server refusal above.
