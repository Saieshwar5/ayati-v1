# Agent computers (proposal, design only)

Every agent can have its own Linux computer for websites, files and programs.
**The computer is permanent; its compute is temporary.** Cuckoo keeps the
computer's disk (files, logins, installed programs) and rents CPU and memory only
while the agent works. It wakes when the agent uses a computer tool and sleeps a
few minutes after. This is CA10 of the earlier build plan ("Computer and
browser"), and it replaces the "Computer" and "Browser" *Coming later* cards in
the approved [Agents](../agents-simplified/README.md) Environment tab. Editable
source: [mockups.html](mockups.html), opened in a browser from this folder. The
example company is fictional and all numbers are examples.

## The idea (owner, 2026-11-08)

1. **Efficient.** Compute runs only while the agent needs it, then is released
   automatically. The environment is kept.
2. **Optional.** Off by default; the agent's owner (or an admin) turns it on.
3. **Bring your own.** A company can connect its own Linux computer instead, in a
   few simple steps.
4. **Provider hidden.** People see *On*, *Connect* and three fixed sizes, never
   who runs the computer.
5. **Provider agnostic.** Several providers from the start, with no dependency
   on any one.
6. **Easy to add a cheaper provider later.**

Owner decisions: start with **E2B, Daytona and AWS Lambda MicroVMs**; **Linux
only** (agents need a browser, files and programs, not a desktop made for
people); keeping data in India matters but is not designed now; **one computer
per agent, with the browser on the same computer** and browsing as a control
(2026-11-08).

| File | Screen and notes |
| --- | --- |
| `1-environment-computer.png` | Agent → Environment. (1) On/Off, off by default. (2) Cuckoo computer or Your own computer; never a provider name. (3) State in plain words; asleep costs nothing; Open computer. (4) Three sizes with the hourly price while working. (5) Browser, files, programs and live view come with it; the Browser card merges into Computer. (6) Everyone who can talk to the agent shares its computer; requests get their own folders and take turns on the screen. (7) Working time, charge and storage this month; Reset computer. |
| `2-computer-states.png` | Off (with the two ways to turn it on); waking then working, with the request and person, Watch and Stop, and the queue; stopped by the monthly budget or by the longest stretch of work; your own computer offline. |
| `3-live-view.png` | Watch the agent's screen, view only by default. (1) Take control / Stop. (2) Take control pauses the agent for passwords or one-time codes; Give back resumes. (3) A payment click held by Controls until a manager approves. (4) Steps with screenshots. (5) Steps stay with the request in Activity. |
| `4-connect-your-own-computer.png` | Name it, run one command with a one-time code, Cuckoo checks the computer, and choose what the agent may do there (its own user and folder; administrator rights off by default). The Connector dials out, so nothing is opened to the internet. |
| `5-admin-computers.png` | Settings → **Computers** (new, admins): allow Cuckoo computers and which sizes; the company's own computers; sleep after, longest stretch and how many may be awake at once; allowed websites; each agent's computer this month. |
| `6-operator-computers.png` | Cuckoo Control → **Computers** (operators only): provider health, wake times, failures, cost and margin with an emergency switch; sizes routed to providers like the model catalog; every computer, its provider and its last disk copy, with Move to… |
| `7-create-connectors-computer.png` | Agent creation, step 2 **Connectors**: the computer sits with the apps as one more thing the agent can use. (1–2) Off by default; switching it on grants nothing yet. (3) Cuckoo computer or a company computer. (4) Only the sizes the admin allows. (5) Browsing is decided in Controls; if it's left off and a skill in step 3 needs a website with no app, the agent asks there. (6) Nothing is made or charged until the agent is created and uses it. |
| `8-create-controls-computer.png` | Agent creation, step 5 **Controls**: a group for the computer beside the apps. (1) **Internet**: no internet, only listed websites, or any website, limited by the admin and enforced on the computer's network. (2) Browser: Allow / Ask first / Block. (3) Clicks that submit are Ask first and money asks the manager. (4) Files, programs and installs are separate controls. (5) Admin limits win. (6) The computer is made on first use. |
| `9-where-the-browser-lives.png` | The three options for the browser (one computer with browsing as a control; two computers per agent; a separate browser service) and how option A enforces "browser off" with two locks: no browser tools, and no internet on the computer's network. |
| `10-chat-browser-use.png` | Chat. (1) After the work: one line, "Used its computer for 3 min", linking to the steps in Activity. (2) While working: one live line with Watch and Stop; the screen is never streamed into Chat. (3) A hand-over is asked in the reply, like a question, and only the person who asked can act. (4) "Keep Atlas signed in", with the warning that everyone who uses the agent shares the login. (5) Replies still go to the agent. |
| `11-activity-hand-over.png` | Activity → Needs me gains **In the browser** beside approvals and questions. The details give what the agent needs with its last screenshot, show that waiting costs nothing, offer Take over the browser or I can't do it, and list the steps so far. |
| `12-hand-over-view.png` | The person uses the agent's browser: just the browser, live, with Paste, I can't do it and **Done, give back**. The agent sees nothing while a person is in control, and that part isn't recorded. The shared-login warning, Keep signed in, the same website rules, and a 15-minute idle limit. |
| `13-live-view-commands.png` | The live view follows the work: the browser's screen only while the agent browses, otherwise **Commands**, with each command, where and when it ran, a short result, files changed and the Controls in force. |
| `14-phone-hand-over.png` | Phone: the notification opens Needs me; the browser fits the phone and uses its keyboard, which suits one-time codes that arrive on that phone. |

## Turning it on while creating an agent

The computer is part of step 2, **Connectors**, next to the apps; there is no
extra step. It is off by default. What the agent may do on it is decided in
step 5, **Controls**, like any app's actions, and suggested from the skills. The
computer itself is made the first time the agent uses it, so an agent that never
needs it never costs anything. Later changes happen on the Environment tab
(computer, size) and the Tools tab (controls).

## Where the browser lives (owner question, 2026-11-08)

Some companies won't let a particular agent browse. The proposal: **one computer
per agent, with browsing as a control**, not two computers and not a separate
browser computer.

- **Browser off** means two locks: the browser tools are never given to the
  agent, and the computer's network allows no websites (or only listed ones). The
  second lock matters because programs on the computer could otherwise reach the
  web without the browser.
- **Why not two computers per agent:** files would live in two places (an invoice
  downloaded in the browser, the spreadsheet on the other computer) and would have
  to be copied across; storage, wake-ups and logins double. Little safety is
  gained, because the same agent drives both.
- **Why not a separate browser computer now:** isolating the browser keeps web
  pages away from the agent's files, but the real danger is a page misleading the
  agent, and Controls and approvals stop that in either design. Cloud browser
  services can be added later as another route behind the same provider
  interface, for a possible "browser only" size.
- Admins can block browsing for the whole workspace in Settings → Computers; agent
  owners can only be stricter.

## Hand-over: when the agent needs a person in the browser

Websites ask for passwords, one-time codes, CAPTCHAs and "Is this you?" checks.
The agent must not get past these on its own, so it hands the browser to a
person.

- **How it starts.** The agent calls a hand-over tool with a reason: sign in,
  type a code, solve a check, or confirm something on the page. Cuckoo also
  forces a hand-over when the agent tries to type into a password field:
  **agents never type passwords**.
- **Who gets it.** The person who asked; for scheduled or event work, the
  agent's owner and managers. It appears in the agent's reply in Chat, in
  Activity → Needs me ("In the browser") and as a notification. In a channel,
  others see "Waiting for Meera to sign in".
- **Waiting is free.** It is saved like a question: the agent holds no model or
  worker, the computer sleeps after the admin's idle time, and the request waits
  up to its time limit (default 24 hours). Taking over wakes the computer on the
  same page. E2B and AWS keep the page in memory; on Daytona, which keeps only
  the disk, the page reloads with the same cookies.
- **While a person is in control,** Cuckoo sends the agent nothing (no
  screenshots, keystrokes or page text) and records nothing. The person sees and
  uses only the browser, with the same website rules. Paste helps with password
  managers. It ends with **Done, give back** or after 15 minutes idle.
- **After.** The agent checks the page and continues. **I can't do it** tells
  the agent, which replies and stops or changes plan.
- **Logins belong to the agent,** shared by everyone who uses it. The hand-over
  says so and asks for the company's account. **Keep signed in** (on by
  default) keeps the cookies; unticked, Cuckoo clears that website's cookies
  when the request ends.
- **Unasked take-over.** Anyone who can stop the request (the person who asked,
  the agent's owner and managers, admins) can also press Take control in the
  live view; the agent pauses the same way.

## What Chat shows

Chat stays a conversation, not a screen. While the agent works, its reply shows
one live line ("using its browser on portal.sharmatraders.in · Watch · Stop").
A hand-over shows as a card in the reply for the person who asked. When the work
is done, one quiet line says how long the computer was used, linking to the steps
in Activity. Screenshots, commands and the live screen live in the live view and
Activity, where the request's own visibility rules already apply. Files reach
people as attachments to the reply or in the Library.

## The live view follows the work

There is no desktop. The browser (Chromium) runs on the agent's Linux computer,
and the live picture is the browser's own screencast through its debugging
protocol (CDP). People's clicks and typing are sent back the same way. So the
screen exists only while the agent is browsing. The rest of the time the live view
shows **Commands** (each command, where and when it ran, a short result, and the
files changed), and nothing runs to draw a screen. This works the same on E2B,
Daytona, AWS and a company's own computer, needs no VNC, and is light enough for
phones.

## Before building: what to understand

1. **Three moving parts.** The agent (on Cuckoo's executors, as today), a
   computer service inside Cuckoo's controller (states, Controls, metering,
   recording, hand-overs, the live picture) and the provider adapters. Inside
   each computer runs Cuckoo's Linux image with Chromium and a small Cuckoo
   helper that runs commands, reads and writes files, and drives the browser.
   The agent never talks to the computer directly; every tool call goes through
   the controller, so Controls apply and the provider stays hidden.
2. **What's in the image.** Ubuntu, Chromium, Python, Node, common document
   tools (PDF text, LibreOffice for Office files), and fonts for Indian scripts,
   so pages and PDFs in Hindi or Tamil render. One image for every provider. When
   Cuckoo updates the image (security fixes), the agent's files, logins and
   programs stay.
3. **What persists.** The whole disk is kept by the provider between wakes, and
   Cuckoo also copies the agent's home folder (files, browser profile) and the
   list of programs it installed to Cuckoo's own storage. That copy is what lets a
   computer move to another provider, and what keeps an AWS computer beyond 8
   hours. The first slice should decide how often to copy (on every sleep is the
   proposal).
4. **One request at a time.** One computer and one browser per agent. Requests
   that need the computer queue for it; requests that don't, never wait. A
   hand-over holds the browser for its request until it ends or times out, so
   other requests for the same agent wait too. Later, separate browser sessions
   per request could let requests overlap.
5. **Shared, not private.** Everyone who uses an agent shares its computer and
   its website logins. Each request gets its own folder, and file tools stay
   inside it unless the agent saves to a shared folder on purpose. This is the
   biggest thing to explain to customers, and the screens say it where it
   matters.
6. **Models and the browser.** The agent reads pages mainly as text (the page's
   structure), which every model can use and which costs little. It uses
   screenshots only when the model can see images (the catalog's "vision"
   ability). So a cheap text-only model can still browse most business
   websites.
7. **Two locks on the internet.** Blocked browser tools are never given to the
   agent, and the computer's own network allows only the permitted websites. How
   each provider blocks outgoing traffic must be confirmed in the real test; if a
   provider can't, Cuckoo routes the computer's traffic through its own filter.
8. **Recording.** Steps, short command results and screenshots are saved with
   the request and follow the workspace's keeping rules. Screenshots can contain
   personal data, so they inherit the request's visibility. Hand-over periods are
   never recorded.
9. **Cost.** Billed per second while awake (wake, work, a few idle minutes), plus
   storage. A hand-over wait costs nothing because the computer sleeps. Charges
   join model charges in Usage and the same budgets.
10. **Testing without paying.** Automated tests use a local test provider (the
    same image on the build machine) so they don't need the cloud. One guarded
    test runs against each real provider before a release.
11. **What's needed to start.** An account and API key for E2B, Daytona and AWS
    (with Lambda MicroVMs in one region), a small spending limit on each for
    testing, and their API hosts allowed in this session's network settings.

## Rules that keep it safe

- **The agent's brain stays outside the computer.** The agent runs on Cuckoo's
  executors as today and drives the computer through tools. Model keys, Cuckoo
  tokens and company rules never sit on the computer. Every click and command is
  an action that Controls and company rules check first, so a payment is held
  for approval. Because the agent isn't inside the computer, the computer can
  sleep while the model thinks, and swapping providers never touches the agent.
- **A computer is untrusted.** Web pages can carry instructions aimed at the
  agent. Allowed websites per workspace, held risky actions and the step log are
  the defences. Nothing on the computer can approve its own actions.
- **One agent, many people.** The computer belongs to the agent, and the
  Environment tab says so. Each request works in its own folder; one request
  drives the screen at a time, and others wait. Requests that don't need the
  computer never wait.
- **Passwords.** Logins are saved in the computer's browser. People type
  passwords and one-time codes themselves with Take control; the agent waits.
  Reset or deleting the agent deletes them.
- **Never hidden from the law.** The product never names the provider, but the
  privacy policy lists each provider as a subprocessor, and Cuckoo needs a data
  processing agreement with each.
- **Deletion.** Deleting an agent or workspace deletes its computer and Cuckoo's
  copy of its disk, except what a legal hold keeps.

## Providers

The research is from 2026-11-08 search results, mostly third-party comparisons.
This session's network blocked the providers' own pages, so every price is to be
confirmed in the first real test. A standard computer (2 CPU, 4 GB) costs about
$0.17 an hour while awake, so an agent working 2 hours a working day costs about
$7.50 a month instead of about $124 always on.

| Provider | Why it's in | Watch out for |
| --- | --- | --- |
| **E2B** (main) | Firecracker microVMs; pause keeps disk, memory and running programs, and resumes in about a second; paused sandboxes are no longer deleted after 30 days; desktop with a screen stream for the live view; Apache-2.0, so it can later run in our own cloud | Continuous run limit per session (pausing resets it); EU region on Pro; no India region found |
| **Daytona** | Stop/archive lifecycle fits exactly (stopped pays disk only, archived to object storage pays nothing); computer use built in; an India region is reported | Containers by default: Cuckoo must use its VM (Kata) class for real isolation; reports in 2026 that it went closed source |
| **AWS Lambda MicroVMs** | Firecracker in Cuckoo's own AWS account, where Cuckoo is planned to run: no third party holds the data, and AWS network rules and billing apply | New (June 2026); 5 regions at launch, none in India; ARM only; suspended memory kept for up to 8 hours, so Cuckoo keeps the disk itself |

Reserves: Fly.io Sprites (persistent, sleeps itself) and Northflank (the lowest
published hourly price, microVMs, bring your own cloud). agent37 is not a fit: it
hosts always-on personal agents per instance, not computers for our own agents.

## How it would be built

- **Computer sizes file**, like the model catalog: *Small* (1 CPU, 2 GB, 10 GB),
  *Standard* (2 CPU, 4 GB, 20 GB), *Large* (4 CPU, 8 GB, 40 GB). Each has private
  routes to providers with priority, weight, cost and price. People see only the
  size.
- **One provider interface, an adapter per provider:** create, wake, sleep, run
  a command, read and write files, screenshot, mouse and keyboard, screen stream,
  export and import the disk, delete. A cheaper provider later is one adapter
  plus a route.
- **Placement and moves.** A new computer is placed by the routes and provider
  health, then stays put. Unlike model calls, a computer can't hop between
  providers per call.
- **Cuckoo keeps the truth.** Every computer starts from Cuckoo's own Linux
  image. The agent's home disk (files, browser profile, programs it installed) is
  copied to Cuckoo's storage when it goes to sleep. That copy is what makes a
  computer movable (Move to…, or automatically after a failed wake) and lets
  Cuckoo keep data the AWS route can't hold for longer than 8 hours. The
  provider's own snapshot is only a fast way back.
- **States:** off, asleep, waking, working, sleeping, resetting, failed; online
  and offline for your own computer. It sleeps when idle for the admin's time
  (default 5 minutes), when the request ends, at the longest stretch of work, or
  when a budget runs out.
- **Agent tools,** only when the computer is on: run a command, files, browser
  (open, read, click, type) and screen (look, click, type). Each is an action that
  Controls and company rules check first.
- **Metering:** seconds awake × size give a charge and a cost, recorded like
  model calls, so they appear in Usage, budgets and Cuckoo Control.
- **Your own computer:** the Cuckoo Connector dials out to Cuckoo over the same
  kind of encrypted, authenticated link that executors use today. It is linked
  with a one-time code, runs as its own user, and can be stopped on the computer.

## Order of work

1. **Real test of the three providers.** Create a computer, install something,
   sign in to a website, sleep it, and wake it a day later. Measure wake time, the
   screen stream and cost. Needs an account and key for each provider, and their
   API hosts allowed in the session's network settings.
2. **First slice:** sizes file, provider interface, E2B adapter, states and
   sleeping, command and file tools, On/Off and sizes on the Environment tab, and
   metering.
3. **Browser and live view:** the browser through CDP, the live picture,
   hand-overs in Chat and Activity, Take control and held clicks.
4. **Daytona and AWS adapters,** disk copies and moves, and the Cuckoo Control
   screen.
5. **Your own computer** and the Connector.
6. **Settings → Computers** and budgets covering computers.

## Behaviour changes to confirm

- The Environment tab's separate Computer and Browser cards become one Computer
  section; Email, WhatsApp and Phone stay *Coming later*.
- A new **Computers** item in Settings under Workspace.
- Budgets cover models and computers together; Usage shows computer time beside
  model use.
- Prices shown to people are per hour while working, in the workspace's currency;
  the margin over provider cost is set in the sizes file, like model prices.
