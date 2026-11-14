# WhatsApp channel (schema 111)

People message an agent on WhatsApp and get its answer there. WhatsApp is **off
by default**. An agent's owner or an admin switches it on with one action, during
creation or later on the agent's Overview, and the agent's number appears at once.
Built 2026-11-10 as milestone W1.

## Decisions

- **Cuckoo provides the numbers.** Cuckoo registers numbers with its WhatsApp
  provider ahead of time and keeps them in a pool. Activation takes a ready one,
  so nobody creating an agent sees Meta, Facebook, SMS codes or business
  verification. The trade-offs: the name people see in WhatsApp is Cuckoo's,
  Cuckoo pays Meta and Twilio per message, and Cuckoo must keep the pool stocked.
  A workspace bringing its own business number (Meta Embedded Signup) is later
  work for Settings, never a step in creating an agent.
- **Twilio is the provider.** Twilio sells numbers and registers them as WhatsApp
  senders through its API, which makes keeping a pool simple. Messages use its
  Messages API, incoming messages and delivery updates its signed webhooks. The
  transport is one gateway adapter (`whatsapp`, server `twilio`), so moving to
  Meta's Cloud API later is a new adapter, not a rewrite. Numbers can be
  migrated between providers.
- **One number per agent.** WhatsApp has no way to address one of several agents
  on a number.
- **The sender's number says who wrote.** A person links their own WhatsApp number
  by sending a code from it. People without a linked number can't message agents
  on WhatsApp: their messages are held without agent work, as for any stranger.

## Switching an agent's WhatsApp on and off

- `POST /api/identity/workspaces/{w}/agent-profiles/{agent}/whatsapp/activate`.
  This needs the agent's owner or an admin, and a saved agent. The new number
  becomes an ordinary live endpoint (`transport` `whatsapp`, `server` `twilio`,
  `address` the number). Repeating the call returns the same number. 409 means
  no number is free; the controller logs a warning so the operator adds numbers.
- `POST …/whatsapp/deactivate` switches the endpoint off. Replies not yet sent
  are cancelled, and the number rests for `cooling_days` (90 by default) before
  another agent can have it, so people who saved it never reach a stranger's
  agent. If the same agent is switched on again while its number rests, it gets
  the same number back.
- `GET …/whatsapp` returns `{offered, state, number, can_manage}`. `state` is
  `off`, `active` or `paused`; `paused` means an admin switched the endpoint off
  with the gateway's endpoint state API. Anyone who may talk to the agent may
  read it, because they need the number to message the agent.
- Each switch is a new generation of the endpoint, so earlier conversations and
  sends never come back.
- A number is never given to a second agent of the same workspace. One address
  never names two of a workspace's endpoints, and old records are immutable.
- A number the operator removed from the file is no longer given out.
- The gateway's generic live endpoint API refuses `whatsapp`, and its binding APIs
  refuse WhatsApp endpoints. The pool and people's own links are the only way in.

## Linking a person's number

1. In Settings › Profile › WhatsApp, the person chooses **Link WhatsApp**:
   `POST /api/identity/mobile/link`. This returns a code such as `LINK-7KQ2MD`,
   a Cuckoo number to send it to and a `wa.me` link that opens WhatsApp with the
   code typed in. The code works for 15 minutes, and only its digest is kept.
2. They send it. Any Cuckoo number accepts link codes. The webhook recognises the
   code, saves the sender's number as theirs (`relay_user_mobiles`) and answers
   "Your WhatsApp number is now linked". The code itself never reaches an agent.
3. **I've sent it** reads `GET /api/identity/mobile`.

Rules:

- One person has one number, and a number belongs to one person. A number already
  linked to someone else, or ever bound to someone else, is refused with a fixed
  reply, because senders are never reassigned.
- Linking a new number closes the old number's conversations.
- `POST /api/identity/mobile/unlink` closes every WhatsApp conversation of that
  number in every workspace and cancels unsent replies. Linking again opens new
  conversations.
- The code is shown only in the person's signed-in session. Whoever sends it links
  their number to that account, so it must not be shared.

## How a message travels

**In.** Twilio posts to `/hooks/whatsapp/twilio`. This is a signed provider
intake, exempt from the browser Origin check like the other `/hooks/` routes.

1. **Signature.** `X-Twilio-Signature` must be HMAC-SHA1 with the auth token over
   the exact URL (`AYATI_PUBLIC_ORIGIN` plus path and query) and every POST
   parameter. Otherwise the answer is 401.
2. **Link code.** If the message is a link code, it is handled as above.
3. **Endpoint.** The number it was sent `To` is looked up in the pool. That
   decides the workspace and agent; the message decides nothing.
4. **Sender.** The number it came `From` is looked up in people's linked numbers.
   - For a workspace member without a binding, a binding is created.
   - A binding whose number no longer belongs to its person is switched off.
5. **Shared intake.** From here the gateway's own intake applies: deduplication
   by Twilio's `MessageSid`, and admission only for an enabled binding of a member
   who may talk to the agent. The person gets one private conversation per agent,
   named WhatsApp, separate from their Chat and from everyone else.
6. **Held messages.** A held sender gets a fixed reply: "This agent can't take
   messages from this number. Link your WhatsApp number in your Cuckoo profile, or
   ask your workspace admin for access." It does not say which of the two applies,
   and the message body is not kept.
7. **Fixed replies.** Fixed replies are TwiML in the webhook's answer. They are
   never agent work.

**Out.** The agent's finished reply goes to the outbox with the room's recipient,
and the gateway sends it as for any live transport:

- Markdown becomes WhatsApp formatting: `*bold*`, `_italic_`, `~strike~`,
  bullets as `•`, links as `text (url)`.
- Long replies are split on line breaks into parts of at most 1,600 characters,
  eight at most.
- Twilio answers are mapped as follows:

  | Twilio's answer | Saved |
  | --- | --- |
  | 201 with a message SID | `accepted` |
  | Another 4xx (for example 63016, outside the 24-hour window) | `failed`, with Twilio's code and message |
  | Connection refused | `queued`, tried again after 30 seconds |
  | Timeout, 429, 5xx, or a later part failing | `uncertain`, never resent |

- Twilio can't be asked about a send by Cuckoo's key, so an unclear send waits for
  a delivery update.
- Each send names `/hooks/whatsapp/twilio/status?workspace=…&key=<attempt>` as its
  status callback. The signature covers that query, so the key can be trusted.
  `delivered` or `read` marks the reply delivered; `failed` or `undelivered` marks
  it failed. A delivered reply is never downgraded.
- Agents keep the WhatsApp manners from schema 52: one complete reply, and no
  "working on it" updates (`send_update` returns `not_here`).

## Setup (operator)

1. **Twilio.** In the Twilio account, buy numbers and register each as a WhatsApp
   sender under Cuckoo's WhatsApp Business Account (console, or the Senders API).
   For each sender:
   - the incoming message webhook is `{AYATI_PUBLIC_ORIGIN}/hooks/whatsapp/twilio`
     (POST);
   - the display name has been approved by Meta.
2. **Token file.** Put the account's auth token in an owned 0600 file.
3. **Operator file.** Write an owned 0600 file named by `AYATI_WHATSAPP_FILE` on
   the controller. Executors refuse it. It is read on every use, so numbers and
   the token can change without a restart:

   ```json
   {"account": "AC…32 hex…", "auth_token_file": "/absolute/private/twilio-token",
    "numbers": ["+14155550142", "+14155550143"], "cooling_days": 90}
   ```

   Its fields:
   - `numbers`: the numbers already registered and approved.
   - `api`: only for local tests, a loopback fake.
4. **Public origin.** `AYATI_PUBLIC_ORIGIN` must be HTTPS. Plain HTTP is accepted
   only on `127.0.0.1` or `localhost` for local tests.

With no file, or no numbers, nothing about WhatsApp is shown, and the routes
answer 404.

## Schema 111

- `relay_whatsapp_numbers`: each pool number's state (`ready`, `assigned`,
  `cooling`), the workspace, agent and endpoint it serves or last served, and when
  it may serve again. There is at most one assigned number per agent.
- `relay_user_mobiles`: a person's verified WhatsApp number. A number is unique
  across people.
- `relay_mobile_link_codes`: digests of unexpired link codes, one per person.

## Checks

- Unit tests:
  - `whatsapp::settings` checks Twilio's documented signature example
    (`L/OH5YylLD5NRKLltdqwSvS0BnU=`), reordered, changed and wrong-key parameters,
    and number format.
  - `whatsapp::text` checks formatting and splitting.
  - `whatsapp_people` checks link code recognition.
- PostgreSQL `communications::whatsapp_tests`:
  - Numbers: given once per agent, owner or admin only, idempotent, none free,
    resting and getting the same number back, never reused within a workspace,
    usable by another workspace, and a number removed from the file.
  - Messages routed by the linked number: a stranger is held with a fixed reply,
    bad and taken codes are refused, rooms are separate per person, duplicates
    are recognised, a member without access is held, and unlinking closes the
    room and cancels the unsent reply while relinking opens a new room.
  - The admin binding API is refused for WhatsApp endpoints.
  - Sending through a local fake Twilio: From, To, converted Body, Basic auth, a
    signed status callback with the attempt, the delivered receipt kept against a
    later failure, and a 63016 refusal saved as failed with its reason.
- Web `tests/whatsapp-channel.test.mjs`:
  - creation activates only when switched on, and the choice is hidden when not
    offered;
  - Overview activates once on a double click, keeps the state on failure, and
    shows the number read-only to people who may only talk to the agent;
  - Profile links with a code and a `wa.me` link, says "not linked yet", and
    unlinks.

## Not built yet (W2–W3)

- **A real Twilio account check.** No live Twilio call or real WhatsApp message
  has been made. Run one end-to-end check with a sandbox or test number before
  launch.
- **Keeping the pool stocked automatically.** Buying a number, registering it
  through the Senders API and waiting for approval. Today the operator adds
  registered numbers to the file.
- **Replies after 24 hours** need an approved template ("Your answer is ready,
  reply to see it"), for example after a long handoff or a schedule. Today Twilio
  refuses them, and they are saved as failed with the reason.
- **Proactive messages** (schedules, events) on WhatsApp.
- **Media.** Pictures, documents and voice notes get "I can only read text
  messages for now."
- **A Communication tab with an approved design.** The number shows on Overview
  and in creation, and linking is in Profile, using existing patterns.
- **Telling a person when their message was held for lack of access.** It is
  shown in the inbox metadata only.
- **Verifying a number that someone else once used.** A recycled number can't be
  linked by its new owner. That needs a support path.
- **Cost reporting** of per-message charges in Usage, and per-workspace limits.
- **A workspace's own number** (Meta Embedded Signup), as an advanced setting.
- **Meta's business-scoped user IDs.** If Meta stops sending phone numbers in
  webhooks, bindings must key on that ID.
