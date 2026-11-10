# Email channel (schema 112)

People email an agent and get its answer in the same email thread. Email is
**off by default**. An agent's owner or an admin switches it on with one action
and the agent gets an address on Cuckoo's mail domain at once, such as
`research-k7q2m@agents.cuckoo.app`. Built 2026-11-10 as milestone E1 of the
[email plan](https://docs.google.com/document/d/16lVfWhDz-j8kx9kg0tjipanehGi9g0uFFX9CaNqWsm8/edit).

## Decisions

- **Cuckoo provides the addresses.** Cuckoo receives every message for one mail
  domain, so an agent's address is a database row: nobody creating an agent
  touches DNS. A workspace's own domain is later work for Settings.
- **Postmark is the provider.** Its inbound webhook delivers the whole parsed
  message, including `StrippedTextReply`; its Email API sends replies; its
  delivery and bounce webhooks return the Metadata Cuckoo gave each send. The
  transport is one gateway adapter (`email`, server `postmark`), so AgentMail or
  Amazon SES later is a new adapter, not a rewrite. AgentMail's own threads and
  inbox storage duplicate what the gateway already keeps, and it is priced per
  inbox.
- **One address per agent, never reused.** A released address keeps its agent,
  which gets it back if email is switched on again. No other agent ever has it.
- **The sender's domain must vouch for the mail.** Anyone can write any `From`
  address, so mail is read only when the sender's own domain authenticated it.
- **The sign-in email says who wrote.** A workspace member's sign-in address is
  their email identity; there is no separate linking step. Email is only as
  trustworthy as the sign-in provider's email verification.
- **Email never gets a fixed reply.** Answering forged or unknown senders would
  send mail to people who never wrote (backscatter). Held and ignored mail is
  recorded without a reply.
- **Each channel keeps its own context, as before.** One email thread is one
  private room. Person memory across Chat, WhatsApp and email is milestone E2
  and needs its own recorded decision.

## Switching an agent's email on and off

- `POST /api/identity/workspaces/{w}/agent-profiles/{agent}/email/activate`
  needs the agent's owner or an admin, and a saved agent. The new address is an
  ordinary live endpoint (`transport` `email`, `server` `postmark`). Repeating
  the call returns the same address.
- `POST …/email/deactivate` switches the endpoint off: mail to the address is
  no longer read and replies not yet sent are cancelled.
- `GET …/email` returns `{offered, state, address, can_manage}`. `state` is
  `off`, `active` or `paused`; `paused` means an admin switched the endpoint off
  with the gateway's state API, or the server no longer serves its domain.
  Anyone who may talk to the agent may read it: they need the address.
- Each switch is a new generation of the endpoint, so earlier conversations and
  sends never come back.
- The address's local part is the agent's name and five random characters.
- The gateway's generic live endpoint API refuses `email`, and its binding APIs
  refuse email endpoints.

## How a message travels

**In.** Postmark posts JSON to `/hooks/email/postmark`. This provider intake is
exempt from the browser Origin check like the other `/hooks/` routes.

1. **Credentials.** The request must carry the Basic credentials from the
   operator file; otherwise 401. Postmark puts them in the webhook URL.
2. **Parse.** A payload Cuckoo can't read is answered 403, so Postmark doesn't
   retry it. Bodies up to 12 MiB are accepted, because Postmark includes
   attachments.
3. **Endpoint.** `OriginalRecipient` is looked up in the agents' addresses.
   That decides the workspace and agent; the message decides nothing.
4. **Readable?** Mail is answered only when all of these hold. Otherwise it is
   recorded as `ignored` and its body is not kept:
   - **Authenticated.** The topmost `X-Spam-Tests` header lists
     `DKIM_VALID_AU` (a valid DKIM signature from the From domain), or the
     topmost `Received-SPF` passes for an envelope sender on the From domain.
     Only the topmost copy counts, because Postmark adds its own above the
     sender's headers. **A live Postmark stream must confirm this before launch.**
   - **Private.** Nobody but the agent is on To or Cc. A Bcc copy addressed to
     someone else counts as shared.
   - **From a person.** Not an auto-reply, bounce or list mail: `Auto-Submitted`
     other than `no`, `Precedence` bulk/junk/list, `List-Id`,
     `List-Unsubscribe`, `X-Autoreply`, `X-Autorespond`, an empty `Return-Path`,
     or a `mailer-daemon`, `postmaster` or `noreply` sender.
   - **Plain text.** `StrippedTextReply`, or `TextBody` with quoted history
     removed. HTML-only mail is not read. Text past 11,900 bytes is cut with a
     note; attachments are mentioned but not read.
5. **Sender.** A readable message's From address is matched to the one
   workspace member whose sign-in email it is, and a binding is created. A
   binding whose address no longer belongs to its person is switched off. Two
   members sharing a sign-in email bind nobody.
6. **Shared intake.** The gateway's intake applies from here: deduplication by
   Postmark's `MessageID`, and admission only for an enabled binding of a member
   who may talk to the agent. Others are `held`.
7. **Room.** One private conversation per agent, sender and thread, named
   Email. The thread is the first ID in `References`, else `In-Reply-To`, else
   the message's own `Message-ID`.
8. **Thread headers.** For admitted mail, `relay_email_messages` keeps the
   Message-ID, up to 20 References and the subject, so the reply can join the
   thread.

**Out.** The agent's finished reply goes to the outbox with the room's sender
as recipient, and the gateway sends it as for any live transport:

- From `"Agent name" <address>`, To the sender, Subject `Re:` the original
  (never stacked), `In-Reply-To` the message answered and `References` its chain.
- The Markdown reply is sent as plain text. Opens and links are not tracked.
- `Auto-Submitted: auto-replied` tells other systems' auto-responders, and other
  Cuckoo agents, not to answer it.
- Metadata `cuckoo_key` (the attempt) and `cuckoo_workspace` come back on
  delivery and bounce events.
- Postmark answers are mapped as follows:

  | Postmark's answer | Saved |
  | --- | --- |
  | 200 with `ErrorCode` 0 | `accepted`, with Postmark's MessageID |
  | Another 4xx (for example 406, inactive recipient; 401, bad token) | `failed`, with the code and message |
  | Connection refused | `queued`, tried again after 30 seconds |
  | Timeout, 408, 429, 5xx, or an unreadable 200 | `uncertain`, never resent |

- Postmark has no idempotency key, so an unclear send waits for an event.
- `/hooks/email/postmark/events` takes the same Basic credentials. `Delivery`
  marks the reply delivered. A `Bounce` that is a `HardBounce` or made the
  recipient inactive marks it failed; other bounces are ignored because the mail
  may still arrive. A delivered reply is never downgraded.
- Agents keep the external manners of schema 52: one complete reply, and no
  "working on it" updates.

## Setup (operator)

1. **Domain.** Choose a mail subdomain, such as `agents.cuckoo.app`, so the
   agents' sending reputation stays apart from the main domain.
2. **Postmark inbound.** Point the subdomain's MX record at
   `inbound.postmarkapp.com`, set it as the inbound stream's domain, and set the
   inbound webhook to
   `https://postmark:{password}@{public host}/hooks/email/postmark`.
3. **Postmark sending.** Verify the subdomain as a sender domain (DKIM and
   Return-Path records), add a DMARC record, and set the delivery and bounce
   webhooks of the outbound stream to
   `https://postmark:{password}@{public host}/hooks/email/postmark/events`.
4. **Secrets.** Put the server API token and the webhook password, at least 16
   characters each, in owned 0600 files.
5. **Operator file.** Write an owned 0600 file named by `AYATI_EMAIL_FILE` on the
   controller. Executors refuse it. It is read on every use:

   ```json
   {"domain": "agents.cuckoo.app",
    "server_token_file": "/absolute/private/postmark-token",
    "webhook_user": "postmark",
    "webhook_password_file": "/absolute/private/postmark-webhook-password",
    "message_stream": "outbound", "sender_name": "Cuckoo agent"}
   ```

   `message_stream` and `sender_name` are optional. `api` is only for local
   tests: a loopback fake.

With no file, nothing about email is offered and the routes answer 404.

## Schema 112

- `relay_email_addresses`: each address, the workspace, agent and endpoint it
  serves, and whether it is `assigned` or `released`. At most one assigned
  address per agent.
- `relay_email_messages`: the Message-ID, References and subject of each
  admitted email, keyed by its inbox receipt.

## Checks

- Unit tests:
  - `email::inbound`: threading from References and In-Reply-To, quote
    stripping, `StrippedTextReply`, aligned DKIM and SPF, a forged header below
    Postmark's, auto-replies and lists, other recipients, long and HTML-only
    mail, and malformed payloads.
  - `email::settings`: address and domain rules, and the webhook's Basic
    credentials, including a secret readable by others.
  - `email::postmark`: sender names can't inject headers.
  - `email_addresses`: local parts from the agent's name.
- PostgreSQL `communications::email_tests`:
  - Addresses: owner or admin only, idempotent, visible to people who may talk
    to the agent, a different address per agent, the same address back after
    switching off and on as a new generation, paused when the domain is no
    longer served, and the binding API refused.
  - Mail: a stranger is held; forged, automated, shared and unreadable mail is
    ignored without keeping its body or starting work; the owner's mail is
    admitted once; a reply stays in its thread's room and a new email opens
    another; a member is held until granted and gets their own room; switching
    off stops reading and nothing is sent afterwards.
  - Sending through a local fake Postmark: the token, From, To, Subject,
    In-Reply-To, References growing along the thread, Auto-Submitted and
    Metadata; delivered kept against a later bounce; a 406 refusal saved as
    failed with its reason.

```sh
RUST_TEST_THREADS=4 cargo test --locked --lib communications::email_tests -- --ignored
```

## Not built yet

- **A live Postmark check.** No real email has been sent or received. Before
  launch, confirm on a real inbound stream which authentication headers
  Postmark adds and that they come first.
- **A Communication tab with an approved design.**
- **Person memory across channels** (E2).
- **Agents emailing teammates** from schedules and events, and a suppression
  list for bounces and complaints (E3).
- **Attachments, safe HTML reading, Cc as a shared room, workspace domains,
  extra verified addresses per person, external recipients** behind approvals,
  and cost reporting (E4).
- **Telling a member why their mail was held.** Held mail has no reply by design;
  it shows in the inbox metadata only.
