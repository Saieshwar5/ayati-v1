# Email channel (schema 113)

People in the organization write to an agent at its own address, for example
`nova.acme@mail.cuckoo.onl`, from Gmail or Outlook, and get the answer in the same
mail thread. Email is **off by default**. An agent's owner or an admin switches it
on with one action, and the address appears at once. Built 2026-11-14 as step E4
of the plan for how work reaches agents (`proj-docs/how-work-reaches-agents-plan.md`).

## Decisions

- **Amazon SES both ways.** Cuckoo already runs on AWS in `ap-south-1`, where SES
  also receives mail. SES stores each incoming mail in a private S3 bucket and
  notifies an SQS queue; replies go out through SES v2 `SendEmail`. Cuckoo runs no
  mail server.
- **A queue, not a webhook.** Controllers long-poll the queue, so there is no
  public mail route, several controllers can read it safely, and mail sent while
  the app is parked waits up to 14 days. A queue message is deleted only after
  Cuckoo saved what it said.
- **One Cuckoo mail domain** on a subdomain, never the root domain: `cuckoo.onl`
  keeps its own mail (today Namecheap's forwarding). Tests use
  `mail.dev.cuckoo.onl` in the existing Route 53 zone.
- **The address is the agent's and workspace's names:** `<agent>.<workspace>@<domain>`,
  lowercase letters, digits and hyphens, `-2`, `-3`… on a clash. It is never given
  to another agent, even after email is switched off, so people who saved it never
  reach someone else's agent. Renaming the agent keeps the address.
- **People don't link anything.** Who wrote is the `From` address, believed only
  when SES's DMARC verdict passed, and only if it is a workspace member's sign-in
  email (verified by WorkOS).

## Switching an agent's email on and off

- `POST /api/identity/workspaces/{w}/agent-profiles/{agent}/email/activate` needs
  the agent's owner or an admin, and a saved agent. Repeating it returns the same
  address. The address becomes an ordinary live endpoint (`transport` `email`,
  `server` `ses`).
- `POST …/email/deactivate` switches it off: mail is held, work that came by email
  loses access and unsent replies are cancelled. Switching on again brings back
  the same address as a new generation.
- `GET …/email` returns `{offered, state, address, can_manage}`; `state` is `off`
  or `active`. Anyone who may talk to the agent may read it.
- The gateway's generic live endpoint and binding APIs refuse email.

## Screens (approved designs in `designs/agent-email/`)

- **Create, Tools step:** Email sits under "Built into Cuckoo", off by default,
  only when the server offers email (the agent directory's `email` is the mail
  domain, or null). Switched on, it previews the address; the agent's address is
  made right after it is created (`web/src/react/agents/email/`).
- **Overview:** an Email row under WhatsApp. Everyone who may talk to the agent
  sees the address and Copy; its owner and admins see Switch on / Switch off.
  Switched off, the row shows the kept address. Hidden when email is not offered
  and the agent never had an address.

## How a mail travels

**In.**

1. SES receives the mail for any address on the domain, scans it for spam and
   viruses, stores it under `in/` in the bucket and notifies the queue (through
   one SNS topic only SES publishes to).
2. The controller reads up to ten notifications per long poll. Spam or a virus is
   dropped. Mail over 10 MB is dropped with a warning.
3. **Address.** The envelope recipient (never the `To:` header), lowercased and
   without a `+tag`, is looked up; that decides the workspace and agent.
4. **Sender.** DMARC must pass and the single `From` address must belong to
   exactly one member of the workspace. A member without a binding gets one. A
   binding whose address now belongs to someone else is switched off, never moved.
   Anything else is held as `unverified:<address>`, which can never match a
   binding: no agent work, and the mail's text is not kept.
5. **Automatic mail is ignored:** `Auto-Submitted` other than `no`, `List-Id`,
   `List-Unsubscribe`, `Precedence: bulk|list|junk|auto_reply`, `X-Autoreply`,
   `X-Autorespond`, an empty return path, and mail from Cuckoo's own domain.
6. **Text.** The plain-text part, or the HTML part as text; the quoted history
   (`On … wrote:`, `>` lines, Outlook's `From:`/`Sent:` block, `-----Original
   Message-----`) and the signature are cut. The room shows the subject in bold
   above the text. Attachments are counted and named in a note, never opened.
7. **Room and thread** ([doors and rooms](communication-gateway.md#doors-and-rooms-schema-112)).
   The mail goes to the person's direct conversation with the agent. A mail thread
   is a thread there: the newest `References`/`In-Reply-To` that names this
   person's earlier mail to the agent, or a reply Cuckoo sent them (SES's ID or
   Cuckoo's own), continues that thread; otherwise the mail starts one. A subject
   alone never joins a thread.
8. Deduplication by SES's mail ID, then the gateway's shared intake.

**Out.**

- The reply goes only to the person who wrote, from the agent's address with the
  agent's current name: `Re:` the subject, `In-Reply-To` the person's mail and
  `References` the thread (at most 20), `Auto-Submitted: auto-replied`, plain text
  and simple HTML from the Markdown (escaped; only `http(s)` links).
- SES replaces the `Message-ID` with its own; the person's client then names SES's
  ID, which Cuckoo recognises through the reply's provider ID.
- Answers from SES:

  | SES's answer | Saved |
  | --- | --- |
  | 200 with a `MessageId` | `accepted` |
  | Another 4xx (for example an unverified address in the sandbox) | `failed`, with SES's message |
  | 403, or connection refused | `queued`, tried again after 30 seconds |
  | Timeout, 408, 429, 5xx | `uncertain`, never resent |

- Each send is tagged with its workspace and attempt. The configuration set's
  `Delivery` event marks the reply delivered; a permanent `Bounce`, a `Complaint`
  or a `Reject` marks it failed. SES suppresses addresses that bounced or
  complained.
- Agents keep the external manners: one complete reply, no "working on it" updates.

## Setup (operator)

1. `deploy/aws/env.sh mail [DOMAIN]` creates the permanent `ayati-mail` stack
   (default `mail.$AYATI_DOMAIN`) and makes its receipt rule set the region's
   active one. It holds:
   - the SES domain identity with Easy DKIM (2048-bit) and the MAIL FROM domain
     `bounce.<domain>`;
   - in the Route 53 zone: MX to `inbound-smtp.ap-south-1.amazonaws.com`, three
     DKIM CNAMEs, the bounce MX and SPF, and `_dmarc` with `p=quarantine`;
   - the bucket `ayati-mail-<account>` (private, 30 days), the topic, the queue
     `ayati-mail` (14 days, five tries, then `ayati-mail-dead`), the receipt rule
     and the configuration set `ayati-mail`.
2. Ask AWS for SES production access in `ap-south-1` (SES console, Get set up).
   Until then SES sends only to verified addresses, at most 200 a day.
3. `env.sh deploy` (or `ship`): the services stack writes the stack's `Settings`
   output to `/run/ayati/email.json` and sets `AYATI_EMAIL_FILE`. The controller
   role may read that queue and the bucket's `in/` and send as the domain.
4. For a domain outside the foundation zone, pass that zone with
   `ZoneId` or add the same records at the registrar.

The operator file (`AYATI_EMAIL_FILE`, controller only, owned 0600, read on every
use) holds no secret; the controller's AWS role signs every call:

```json
{"domain": "mail.dev.cuckoo.onl", "region": "ap-south-1",
 "queue_url": "https://sqs.ap-south-1.amazonaws.com/<account>/ayati-mail",
 "bucket": "ayati-mail-<account>", "configuration_set": "ayati-mail"}
```

`api` (`{"ses": …, "sqs": …}`) is only for local tests, a loopback fake. With no
file nothing about email is shown, and activation answers 404.

## Schema 113

- `relay_email_addresses`: each address, its domain, workspace, agent and
  endpoint. Immutable; one per agent and domain.
- `relay_email_messages`: each admitted mail's sender, `Message-ID`, subject,
  references, its message in the room and the room thread it belongs to.
  Immutable; held and ignored mail leaves none.
- An index on the outbox's provider ID, to find a reply by SES's ID.

## Checks

- Unit: parsing (sender, thread headers, quoted history, automatic mail, HTML-only,
  long mail), SES notifications and events, the composed reply (threading,
  escaping, no header injection, parsing back), SES answers, address names and
  the settings file.
- PostgreSQL `communications::email_tests`: addresses given once and kept,
  owner/admin only, a second agent of the same name, the admin binding API
  refused; forged, outside, automatic and unknown-address mail getting no work;
  the member's mail (also to a `+tag`) in their direct room; reading the fake
  queue and S3, deleting only after saving, sending through fake SES, the delivery
  event, a follow-up continuing the room thread by SES's ID, a bare subject
  starting a new one, and switching off cancelling the unsent reply.
- Web `tests/email-channel.test.mjs`: creation activates only when switched on,
  with the address preview; no choice when not offered; Overview switches on once
  on a double click and keeps the state on failure; Copy, the kept address after
  switching off, and nothing when not offered.
- Checked in a browser (local simulation, 2026-11-14): create with email on, the
  address on Overview, Switch off keeps it, Switch on brings it back.
- Not yet: a live SES check (domain verification, a real Gmail and Outlook
  thread, a forged sender, a bounce).

## Not built yet

- Attachments, several recipients and CC (needs groups outside Chat, E6), mail an
  agent starts itself, a workspace's own domain, and people outside the
  organization.
- A notice through the door when a request waits for an answer inside Cuckoo.
