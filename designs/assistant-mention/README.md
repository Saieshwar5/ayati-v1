# Asking your assistant with a mention (2026-11-21)

**Status: built 2026-11-21**, with the three recommended choices, after the owner
said to work on it. What is built, its checks and its limits are in
[`runtime/assistants.md`](../../runtime/assistants.md#asked-from-any-message-box-2026-11-21).

The idea: instead of the Draft button, a person mentions their own assistant in any
conversation and says what to do. The assistant writes a draft that only its person
sees; the person sends it as their own message, or discards it.

Open [`mockups.html`](mockups.html) for the screens.

What differs from the screens:

- **The card does not show what the assistant used**, nor the step it is on while it
  writes ("Reading Sales sheet…"). Both need the assistant to report them; today a
  draft is only its text. The card shows what you asked instead.
- **The server refuses only a real mention.** A name typed by hand is plain text, so
  the warning in the message box is the only guard for that case.

## The design in short

- **Any name.** "Ayati" is only the name an assistant has until its person renames it.
  A mention holds the assistant's ID. The @ list shows a person only their own
  assistant, first, marked "Your assistant"; `@assistant` and `@my` find it too.
- **The message box changes** once the assistant is mentioned: purple border, the line
  "Asking <name> · Only you see this. Nothing is posted in #channel", and the button
  reads "Ask <name>". A team agent's mention stays public and unchanged.
- **The instruction is never posted.** The client sends such a text to the assistant
  only, the box warns when the name was typed by hand, and the server refuses a posted
  message that mentions the sender's own assistant.
- **A private draft card** above the message box: where it will be sent and as whom,
  the text, what the assistant used, then Send as me, Edit, Discard and "Ask <name> to
  change it". The box stays free while the assistant works.
- **Sent as the person.** Others see an ordinary message. The private mark, the Audit
  line and learning from edits stay as built.
- **"@<name> respond"** answers the newest message meant for the person; the card
  quotes what it answers.
- **A stale draft** (new messages since it was written) says so and offers Update.
- **No wording grants sending without review.** Take over stays its own action.

## Choices made

1. A card above the box, replacing the 2026-11-19 rule against a row above the
   message box (`runtime/design-system.md` says so).
2. "Respond" answers the newest message that mentions the person.
3. The Draft button stays as a shortcut that only inserts the mention.

## Not built

What the assistant used and what it is doing; a draft kept across devices or past an
hour; files as part of what is asked. See the limits in `runtime/assistants.md`.
