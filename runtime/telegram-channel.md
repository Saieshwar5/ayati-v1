# Telegram channel (schema 113)

People in a workspace talk to an agent on Telegram and get its answers there.
Every agent keeps Cuckoo Chat by default; Telegram is **off** until the agent's
owner or an admin switches it on. Each agent gets **its own Telegram bot**. Only
members who connected their own Telegram account can talk to it: Telegram is a
channel for people inside the organisation, never for outsiders. Built
2026-11-14 as T1 of the
[Telegram plan](https://docs.google.com/document/d/1l97uinCQEzlM4Fbjz1RGg8wCecPuPY0BVYQxRpRkvGY/edit).
WhatsApp is postponed: its code stays, hidden while no `AYATI_WHATSAPP_FILE` is set.

## Decisions

- **Cuckoo runs one manager bot.** It creates and manages every agent's bot
  through Telegram's managed bots (Bot API 9.6). Nobody uses BotFather or copies
  a token.
- **A person creates each bot with one tap.** Telegram lets only a person create
  a bot, so no bot is created silently. The admin who connected the workspace's
  Telegram is its **Telegram owner**: they confirm each agent's bot in Telegram.
  Logging a person's Telegram account into Cuckoo to skip the tap was rejected:
  it would give Cuckoo their whole Telegram.
- **No bot token is stored.** The manager bot asks Telegram for a managed bot's
  token (`getManagedBotToken`) when it is needed and keeps it only in memory. The
  manager token is the one secret, in the operator's file.
- **The Telegram user ID says who wrote.** A person links their account once by
  pressing Start on the manager bot with a code from their signed-in session.
  Display names and usernames are ignored. The same ID reaches every bot, so one
  link serves every agent and workspace.
- **Private chats only.** Groups are left; approvals never happen over Telegram.
- **One bot per agent, forever.** Switching off and on brings the same bot back
  as a new generation; no other agent can ever have it.

## Connecting a workspace

`POST /api/identity/workspaces/{w}/telegram/connect` (admins only) returns
`https://t.me/<manager>?start=<code>`. Pressing Start links the admin's account
and makes them the Telegram owner; another admin doing the same takes over.
`GET …/telegram` returns `{offered, connected, owner, is_owner, can_manage}`.
`connected` is false when the owner later disconnected their Telegram.

## Switching an agent's Telegram on and off

- `POST …/agent-profiles/{agent}/telegram/activate` needs the agent's owner or an
  admin, a saved agent and a connected workspace (otherwise 409). It records a
  request with a suggested username (the agent's name, five random characters,
  `_bot`) and returns `state: requested`.
  - The Telegram owner sees `create_link`,
    `https://t.me/newbot/<manager>/<username>?name=<agent> · <workspace>`, on
    Overview and taps Create.
  - When someone else switched it on, the manager bot sends the owner a message
    with a "Create the agent's bot" button (`request_managed_bot`).
    `owner_asked` says whether it was sent.
- The manager bot then receives `managed_bot`. The new bot is matched to the
  creator's open request: the one whose suggested username it kept, else their
  latest. It becomes a live endpoint (`telegram`, server `telegram`, address the
  bot's numeric ID), and Cuckoo sets its description and `/start` command.
  A bot made by anyone else, or with no open request, is not attached.
- `POST …/telegram/deactivate` switches the endpoint off and cancels unsent
  replies, or drops a request whose bot doesn't exist yet.
- `GET …/telegram` returns `{offered, connected, state, username, link,
  create_link, is_owner, can_manage}`. `state` is `off`, `requested`, `active`
  or `paused` (an admin switched the endpoint off). Anyone who may talk to the
  agent may read it.
- The gateway's generic endpoint API refuses `telegram`, and its binding APIs
  refuse Telegram endpoints.

## Linking a person's account

- `POST /api/identity/telegram/link` returns a Start link with a code valid for
  15 minutes, single use, kept only as a digest.
- Pressing Start saves the account as theirs; the manager bot confirms by name.
- `GET /api/identity/telegram` returns `{offered, linked, username}`.
- `POST /api/identity/telegram/unlink` closes every Telegram conversation of that
  person and cancels unsent replies.

Rules: one person has one account; an account once linked to someone is never
moved to another person, even after unlinking. Linking a new account closes the
old one's conversations.

## How a message travels

**In.** Every 3 seconds each controller takes a 30-second lease on the manager
bot and on each active bot and reads `getUpdates` from its saved position. The
position moves only after the updates are saved, so a crash rereads them;
duplicates are dropped by update ID. Polling needs no public address.

For each private message to an agent's bot:

1. **Which agent.** The bot decides the workspace and agent; the message decides
   nothing.
2. **Who wrote.** The sender's Telegram ID is looked up in people's linked
   accounts. A linked workspace member gets a binding. A binding whose account no
   longer belongs to its person is switched off.
3. **May they.** The gateway's intake admits only an enabled binding of a current
   member who may talk to the agent. One private conversation per person and
   agent, named Telegram, separate from their Chat.
4. **Fixed replies, never agent work.**
   - A refused sender gets "This assistant is only for members of {workspace} who
     may use it. Connect Telegram in your Cuckoo profile, or ask your workspace
     admin for access."; the body is not kept.
   - `/start` greets a member by name, or refuses.
   - Pictures, voice and files get "I can only read text messages for now."
   - Messages in groups make the bot leave.

**Out.** The agent's finished reply goes to the outbox with the room's chat and
is sent through the agent's own bot:

- Markdown becomes Telegram HTML: bold, strike, code, code blocks, links (http
  and https only), headings in bold, bullets as `•`. Everything else is escaped.
- Long replies are split on line breaks into parts of at most 4,000 characters,
  eight at most.
- If Telegram refuses the formatting, the same part is sent once as plain text
  (a refusal means nothing was sent).

| Telegram's answer | Saved |
| --- | --- |
| `ok` with a message ID | `accepted` (bots get no delivery receipts) |
| 403, e.g. the person blocked the bot | `failed`, with the reason |
| 401 | `failed`; the token is asked for again next time |
| Other 4xx | `failed`, with Telegram's description |
| 429, or connection refused | `queued`, tried again after 30 seconds |
| Timeout, 5xx, or a later part failing | `uncertain`, never resent |

Agents keep the external manners of schema 52: one complete reply, no "working
on it" updates.

## Leaving and losing access

Access is checked on every incoming message and again before every send:

- **Removed from the workspace:** their next message is refused and unsent
  replies are cancelled.
- **Access to an agent removed:** that agent refuses them.
- **Telegram switched off for the agent:** its bot is not read.
- **Unlinking:** closes the person's conversations everywhere.

Cuckoo cannot remove messages already on a person's phone. Telegram lets a bot
delete its own messages only within 48 hours.

## Setup (operator)

1. Create the manager bot with @BotFather, then turn on **Bot Management Mode**
   for it in BotFather's Mini App, so other bots can be created for it to manage.
2. Put its token in an owned 0600 file.
3. Write an owned 0600 file named by `AYATI_TELEGRAM_FILE` on the controller
   (executors refuse it). It is read on every use:

   ```json
   {"manager_username": "CuckooAgentsBot", "manager_token_file": "/absolute/private/telegram-manager-token"}
   ```

   `api` is only for local tests: a loopback fake.
4. Allow outbound HTTPS to `api.telegram.org` from the controller.

With no file, nothing about Telegram is shown and the routes answer 404.

## Schema 113

- `relay_telegram_workspaces`: each workspace's Telegram owner.
- `relay_user_telegram`: a person's linked account (unique both ways).
- `relay_telegram_accounts`: every account ever linked, and to whom.
- `relay_telegram_link_codes`: digests of unexpired Start codes.
- `relay_telegram_bots`: each agent's bot or open request, its read position and
  lease.
- `relay_telegram_manager`: the manager bot's read position and lease.

Schema 112 is reserved for the email channel on its branch.

## Checks

- Unit tests:
  - `telegram::settings`: usernames, tokens, loopback-only API.
  - `telegram::text`: HTML conversion, escaping, unsafe links, splitting, closed
    code blocks.
  - `telegram_bots`: suggested usernames and bot names.
  - `telegram_people`: Start code shape.
- PostgreSQL against a local fake Bot API:
  - `communications::telegram_tests`:
    - connecting (admins only);
    - activation (owner or admin, off by default, 409 before connecting);
    - the owner's link and the button sent to the owner;
    - a stranger's bot not attached;
    - the bot profile set;
    - one bot per agent, and off/on as a new generation;
    - replies sent as HTML to the room's chat, plain-text fallback, a blocked bot
      saved as failed;
    - the binding API refused.
  - `communications::telegram_people_tests`:
    - strangers and members without access refused before agent work;
    - linking once, `/start` greetings, separate rooms;
    - rereading updates admits nothing twice;
    - groups left, non-text answered;
    - an account taken by someone else;
    - leaving the workspace and unlinking stop it.
- Web `tests/telegram-channel.test.mjs`:
  - the create step (off by default, disabled before connecting);
  - Overview (one activation on a double click, failure keeps it off, Create bot
    for the owner, Open in Telegram and Connect for others, nothing when not
    offered);
  - Profile connect and disconnect;
  - Settings › Workspace connect.

```sh
RUST_TEST_THREADS=4 cargo test --locked --lib telegram -- --include-ignored
```

## Not built yet

- **A live check** with a real manager bot and two Telegram accounts. In
  particular, confirm that a bot created from the link or button arrives as
  `managed_bot` with the suggested username, and that `getManagedBotToken` works.
- **Connecting Telegram while joining a workspace** (today: Profile and the
  agent's Overview).
- **Voice notes and files**, through the existing transcription and Library.
- **Messages from automations** to a person on Telegram.
- **"Typing…"** while the agent works.
- **An admin view of who connected Telegram**, and a count of refused messages
  in Activity.
- **Webhooks** when the number of bots makes polling wasteful.
- **Removing the bot from Telegram when an agent is deleted.** Today its
  endpoint is simply no longer read.
- **Person memory across Chat, Telegram and email** (D247, E3).
- **A Communication tab with an approved design.**
