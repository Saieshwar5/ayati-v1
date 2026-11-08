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
people); keeping data in India matters but is not designed now.

| File | Screen and notes |
| --- | --- |
| `1-environment-computer.png` | Agent → Environment. (1) On/Off, off by default. (2) Cuckoo computer or Your own computer; never a provider name. (3) State in plain words; asleep costs nothing; Open computer. (4) Three sizes with the hourly price while working. (5) Browser, files, programs and live view come with it; the Browser card merges into Computer. (6) Everyone who can talk to the agent shares its computer; requests get their own folders and take turns on the screen. (7) Working time, charge and storage this month; Reset computer. |
| `2-computer-states.png` | Off (with the two ways to turn it on); waking then working, with the request and person, Watch and Stop, and the queue; stopped by the monthly budget or by the longest stretch of work; your own computer offline. |
| `3-live-view.png` | Watch the agent's screen, view only by default. (1) Take control / Stop. (2) Take control pauses the agent for passwords or one-time codes; Give back resumes. (3) A payment click held by Controls until a manager approves. (4) Steps with screenshots. (5) Steps stay with the request in Activity. |
| `4-connect-your-own-computer.png` | Name it, run one command with a one-time code, Cuckoo checks the computer, and choose what the agent may do there (its own user and folder; administrator rights off by default). The Connector dials out, so nothing is opened to the internet. |
| `5-admin-computers.png` | Settings → **Computers** (new, admins): allow Cuckoo computers and which sizes; the company's own computers; sleep after, longest stretch and how many may be awake at once; allowed websites; each agent's computer this month. |
| `6-operator-computers.png` | Cuckoo Control → **Computers** (operators only): provider health, wake times, failures, cost and margin with an emergency switch; sizes routed to providers like the model catalog; every computer, its provider and its last disk copy, with Move to… |

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
3. **Browser and live view,** with Take control and held clicks.
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
