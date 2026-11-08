# Agent computers (schema 103)

An agent can have its own Linux computer for work with files and programs: reading a
PDF or spreadsheet, converting a file, working through data or running a script. It
is off for every agent until its owner or an admin turns it on. Its compute runs only
while the agent works; asleep, it keeps its files and costs nothing. People never see
which provider runs it. Designed in [designs/agent-computers](../designs/agent-computers/README.md);
the provider check is in [agent-computer-providers](agent-computer-providers.md).
First slice (server and screens) built 2026-11-08.

## Turning it on

| Where | Setting |
| --- | --- |
| Controller | `AYATI_COMPUTER_PROVIDERS_FILE`: an absolute path to the JSON file below. Without it, no agent can have a computer, and turning one on is refused. |
| Agent | `settings.computer`: `on`, `size`, `programs`, `internet` and `websites`, saved with the agent by its owner or an admin. |

```json
{
  "sizes": [
    {"size": "small", "cpu": 1, "memory_gb": 2, "disk_gb": 10, "usd_per_hour": 0.09,
     "routes": [{"provider": "e2b", "image": "cuckoo-computer-small", "usd_per_hour": 0.06}]},
    {"size": "standard", "cpu": 2, "memory_gb": 4, "disk_gb": 20, "usd_per_hour": 0.17,
     "routes": [{"provider": "e2b", "image": "cuckoo-computer-standard", "usd_per_hour": 0.1}]}
  ],
  "providers": [{"provider": "e2b", "key_file": "/run/secrets/e2b"}],
  "sleep_after_minutes": 5,
  "longest_work_minutes": 120
}
```

- `sizes` are what people choose: `small`, `standard` or `large`, each with
  1–16 CPUs, 2–64 GB memory and 5–200 GB disk. `usd_per_hour` is what people are
  charged per hour awake.
- `routes` are private to operators: which provider image serves a size and what it
  costs Cuckoo per hour. New computers use the first route. `image` is an E2B
  template built from [deploy/agent-computer](../deploy/agent-computer/Dockerfile).
- Each `key_file` holds only the provider's API key. It is read on every use, so it
  can be rotated in place. Keys stay on the controller: executors and agents never
  get them. `base_url` and `computer_url` are optional HTTPS addresses; loopback
  HTTP is accepted for local checks only.
- A computer sleeps `sleep_after_minutes` (1–120) after its last use, and never stays
  awake longer than `longest_work_minutes` (5 minutes to 24 hours) in one go.
- The controller checks the file at start and refuses to start if it is broken.

## What agents get

Four tools of Cuckoo's own, and one system prompt section, **Its computer**, when its
computer is on:

- **`computer_run({command, seconds?})`** runs one shell command (up to 4,000
  characters) in this request's folder and returns the exit code, the start of its
  output (8,000 characters) and of its errors (2,000). It stops after `seconds`
  (default 120, at most 600).
- **`computer_list_files({path?})`**, **`computer_read_file({path})`** (text up to
  30,000 characters) and **`computer_write_file({path, content})`** (up to 1 MB).

Each request works in `requests/<reply id>` in the agent's home folder. Paths start
there; `shared/` is kept for every request. Absolute paths and `..` are refused. The
provider's name never appears in a tool result.

## Rules Cuckoo enforces

- **One computer per agent, shared by everyone who uses the agent.** One request
  holds it at a time; a hold lapses three minutes after its last use, and ends when
  the request finishes. Another request is told the computer is busy.
- **Programs:** `allow` (default), `ask` or `block`. With `ask`, each new command is
  put to the person who asked, as a yes/no question; only their yes to that exact
  command lets it run. With `block`, only the file tools work.
- **Internet:** `none` (default), `listed` (only `websites`, up to 30 host names such
  as `portal.example.in` or `*.example.in`) or `any`. The provider enforces it on the
  computer's network; a changed setting applies on the next use.
- **Budgets.** Computer charges count towards the same monthly budgets as models.
  At the limit the computer isn't woken.
- **Size.** A size chosen later applies to the next computer made: after a reset, or
  if the first one hasn't been made yet.

## Lifecycle

1. **First use:** Cuckoo creates the computer from the size's image with the agent's
   internet rule, and starts an awake stretch.
2. **Later use:** Cuckoo wakes it (files, memory and running programs are kept) and
   renews its access every minute while in use.
3. **Sleep:** every 30 seconds a controller task puts idle computers to sleep, ends
   stretches that ran past the longest work time, and charges each stretch: seconds
   awake × the size's price, with our cost recorded beside it.
4. **Reset** (owner or admin, audited): the computer is deleted and the next use
   starts a new one. A computer the provider lost is replaced, empty, on its next use.

## Records

| Table | What it holds |
| --- | --- |
| `relay_computers` | One row per agent: size, provider, image, the provider's id, `asleep`/`awake`/`deleting`, the internet rule applied, who holds it and until when, and the last problem. |
| `relay_computer_sessions` | Each awake stretch: agent, request, person, size, seconds, charge and cost. |
| `relay_computer_steps` | What the agent did for a request: each command with its exit code and output (up to 140,000 characters), and each file listed, read or saved. Deleted with the reply. |

## Where it shows

- **API:** `GET …/agent-profiles/{id}/computer` gives anyone who can see the agent its
  setting, the computer's state and this month's awake time and charge.
  `POST …/computer/reset` is for the owner or an admin. The agents list carries
  `computers`: the sizes with their hourly price, or `null` when not set up.
- **Activity:** each step in a request's details, `effect: "computer"`, with the
  command's exit code and the start of its output.
- **Usage:** `computers` beside model calls, by agent, and `total_charge_usd`.

Screens (designs/agent-computers 1, 7 and 8):

- **Agent → Environment → Computer**, when this Cuckoo offers computers: the switch,
  its state in plain words (not made yet, asleep, working, being reset), the sizes
  with their hourly price while working, this month's working time and charge, and
  Reset computer with a confirmation. A size chosen after the computer was made
  says it applies after a reset. Read-only for everyone but its owner and admins.
- **Agent → Tools:** the computer's Internet (no internet, only listed websites, any
  website) and Run programs (Allow, Ask first, Block), saved together.
- **Create, step 2 (Tools):** a Computer switch with the size, off by default.
  **Step 5 (Controls):** the same Internet and programs controls when it is on.
- **Activity → request details:** each command with how it finished and its output
  behind a disclosure, files listed, read or saved, and commands that asked first.
  Asking first is a question to the person, not an approval.
- **Usage:** computer time and charges beside model use, and by agent.

## Adding a provider

1. Add one file in `src/identity/computers/` that implements `Provider`: create,
   wake, sleep, delete, set the internet rule, run a command, and read and write a
   file. A computer the provider no longer has must fail wake with "no longer exists".
2. Add one line to `adapter()` in `src/identity/computers/mod.rs`.
3. Add routes for it in the providers file.

## Checks

- `cargo test computers`: the providers-file checks, and the full lifecycle through
  the E2B adapter against a local stand-in (`computers/fake.rs`) whose computers
  are folders where commands really run.
- `cargo test the_real_e2b -- --ignored` with `E2B_API_KEY` set: the same lifecycle on
  E2B (template `AYATI_COMPUTER_TEST_IMAGE`, default `cuckoo-computer-dev`). Passed
  on 2026-11-08.
- `cargo test -p ayati-guest computer`: the forwarded tool fields.
- Web (`web/tests/agent-computers.test.mjs`): turning it on once and keeping it off
  after a failed save; the state, sizes, month and reset; read-only viewers; Tools
  controls and website checks; the create flow on and off; Activity steps; Usage.
- PostgreSQL (`postgres::computer_tests`):
  - off until turned on; commands, files and the request's folder; paths outside it
    refused; the default network rule; Activity steps; the Environment status;
  - requests taking turns; a changed internet rule; the sweep sleeping an idle
    computer and charging it; Usage; files kept after sleep; the budget stopping it;
  - a lost computer replaced; reset by the owner only, audited, at the new size;
  - programs blocked and asked first; nothing turned on without a providers file.

## Not yet

- The browser, the live view and the browser hand-over, and Chat's one line
  ("Used its computer for 3 min") linking to the steps.
- Moving a computer to another size or provider with its files, Daytona and AWS,
  the Cuckoo Control computers page and the own-computer connector.
- Workspace policy (allowed sizes, an admin off switch) and Settings → Computers.
