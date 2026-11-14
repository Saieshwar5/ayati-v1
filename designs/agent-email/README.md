# Agent email screens (proposal, 2026-11-14)

For the owner's approval before building. Built on the approved Agents designs
(`designs/agents-simplified/agents.fig`) and the WhatsApp screens already in the
app. Runtime behaviour: `runtime/email-channel.md` (E4). The example company is
fictional.

| File | Screen |
| --- | --- |
| `1-overview.png` | Overview: the agent's address in its key facts, under WhatsApp, with Copy and Switch off |
| `2-create-tools.png` | Create, Tools step: Email beside Web search and WhatsApp under "Built into Cuckoo", off by default |
| `3-overview-states.png` | The Email row: owner with email off, someone who can only talk to the agent, switched off after use |
| `4-chat-and-mail.png` | Mail in the person's direct conversation (subject in bold, "via email", a mail thread as a Chat thread), the same conversation on WhatsApp, and the answer in their mail app |

## What the screens decide

- **Where email is switched on:** at creation (Tools step, with the other
  built-in tools) and later on Overview. Only the agent's owner or an admin.
  Shown only when this Cuckoo server offers email.
- **The address is shown, never typed:** `<agent>.<workspace>@<domain>`, made
  when the agent is created; a second agent of the same name gets `-2`.
- **Who sees it:** everyone who may talk to the agent sees and copies it.
- **No setup for people:** unlike WhatsApp there is nothing to link; people write
  from the address they sign in with. One line on Overview says so.
- **Switching off keeps the address** for the same agent; mail gets no answer
  while it is off.
- **Chat already shows "via email"** (built in E3); screen 4 shows it with a
  mail thread.

## Not in these screens

Attachments, CC, a workspace's own domain and people outside the organization
(later steps).

`agent-email.fig` holds the four screens; the builder is kept with the other
private design builders (`proj-docs/design-builders/agent-email/build.py`).
