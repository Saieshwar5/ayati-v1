# Phone layout and Chat speed checks

Two repeatable browser checks run against the isolated company simulation
([company-simulation.md](company-simulation.md)): a layout audit of the main screens at
fixed phone and tablet sizes, and a Chat speed baseline compared with a committed
baseline file. Both use the existing Playwright install in `web/node_modules`, sign in
through the simulation-only employee login and write their reports under the run
directory in `.ayati-local/company-simulation/runs/` (ignored by Git).

## Commands

Start the simulation and seed a run first (`setup`, `serve`, `run`; see
[company-simulation.md](company-simulation.md)), build the client
(`npm run build --prefix web`), then:

```sh
./tools/company-simulation.sh layout            # all screens, 9 sizes, light and dark
./tools/company-simulation.sh layout --sizes 320x568,390x844 --screens channel,thread --themes dark
./tools/company-simulation.sh speed             # 5 measured repetitions, compare with the baseline
./tools/company-simulation.sh speed --strict    # exit 1 when a metric regresses
./tools/company-simulation.sh speed --update-baseline
```

The subcommands call `node tools/company-simulation/mobile-layout.mjs` and
`node tools/company-simulation/ui-performance.mjs` with the simulation's runs directory;
both scripts also accept another runs directory as their first argument, like the other
simulation scripts. Other options: `--no-screenshots` (layout), `--repeat N`,
`--cpu-throttle N` and `--baseline path` (speed). Set `AYATI_CHROMIUM` when Chromium is
not at `/usr/bin/chromium`.

The layout audit exits 1 when any screen fails a rule. The speed check only warns
unless `--strict` is given. Each check adds a few retained fixture conversations
(found again by name on later runs): `Layout audit · phone review` with a thread and the
owner's direct conversation with their team agent, and `Speed check · channel A`/`B`
(40 messages each) and `Speed check · 200 messages`.

## Layout audit

**Screens** (signed in as the Owner, employee 0): Home, Chat list, a channel
conversation, its thread panel (opened with a tap on the reply count), a direct
conversation with an agent, Agents, an agent's profile, Activity, Apps, Library,
Settings (the page list), Settings → Profile and Settings → People.

**Sizes** (CSS pixels, device scale factor 2, touch and mobile emulation so
`pointer: coarse` styles apply):

| Size | Device class |
| --- | --- |
| 320×568 | small phone (iPhone SE 1st generation, small Android) |
| 360×740 | compact Android |
| 375×667 | iPhone SE 2nd/3rd generation |
| 390×844 | iPhone 12–14 |
| 393×852 | iPhone 15/16 |
| 412×915 | large Android (Pixel) |
| 430×932 | iPhone Pro Max |
| 768×1024 | iPad mini (tablet, portrait) |
| 820×1180 | iPad Air (tablet, portrait) |

Each size runs in the light and dark themes (the stored theme preference plus the
matching `prefers-color-scheme`), so a full run is 13 screens × 9 sizes × 2 themes = 234
screen checks.

**Rules** (`tools/company-simulation/layout-rules.mjs`, evaluated in the page):

| Rule | Fails when |
| --- | --- |
| `horizontal-scroll` | `document.documentElement.scrollWidth > innerWidth` |
| `touch-target` | a shown `button`, `a[href]`, `input`, `select`, `textarea`, `[role=button]`, `[role=switch]` or `[role=tab]` has a touch box under 44×44 CSS px |
| `clipped-text` | an element with `overflow-x: hidden/clip` and text has `scrollWidth > clientWidth + 1`, no `text-overflow: ellipsis` and no `title` on itself or an ancestor; the report names the descendant reaching furthest right |
| `past-right-edge` | an element starts inside the viewport but its right edge is more than 1 px past it, and neither a clipping ancestor nor a local horizontal scroller contains it (only the outermost such element is listed) |
| `screen-unavailable` | the screen did not open (its heading, list or composer never appeared) |

Deliberate simplifications, to keep the heuristic predictable:

- "Shown" means at least 2×2 px, not `opacity: 0`/`visibility: hidden`, and not fully
  cut off by an `overflow: hidden/clip` ancestor or outside the viewport horizontally
  (off-canvas panels). Content further down a scroll container still counts.
  Controls revealed only on hover or tap (such as the message actions toolbar) are
  not audited until shown; the `clipped-text` rule still notices when they overflow.
- A form control's touch box includes its `<label>` elements. Hit areas enlarged with
  pseudo-elements or padding on a parent are not detected.
- An open thread panel (a modal drawer before schema 115) is audited on its own; the page behind it is
  covered by the channel screen.
- Allowlist: inline `a` elements inside running text (the WCAG 2.5.8 inline exception,
  such as links in messages) and the simulation-only "Switch employee" banner, which
  real workspaces never show.

The console summary groups identical problems (rule, screen, element, region) with the
sizes and themes where they occur and example accessible names. `report.json` holds every
finding with its box; failing screens get a screenshot with offenders outlined in red.

## Chat speed baseline

Measured at 1366×900 in headless Chromium with a warm HTTP cache, after one unrecorded
warm-up pass. Each value is the median of `--repeat` passes (default 5). Timings are
taken inside the page: the start is `performance.now()` in the same task as the click
(or navigation start for a full load) and the end is the first animation frame in which
the target is inside the viewport (`waitForFunction` with `raf` polling).

| Metric | What is timed |
| --- | --- |
| `chat_open_cold_ms` | full page load of `?view=chat` until a conversation link shows in the list |
| `chat_open_from_home_ms` | clicking Chat in the rail on Home until the list shows (in-app navigation) |
| `conversation_switch_ms` | clicking another 40-message channel until one of its messages shows (A→B and B→A) |
| `large_conversation_ms` | clicking the 200-message channel until its transcript shows |
| `*_long_tasks` | `longtask` entries (>50 ms main-thread tasks) starting inside each interval |
| `bundle_entry_gzip_kB` | gzip level 9 of the JS/CSS that `web/dist/index.html` loads |
| `bundle_total_gzip_kB` | gzip level 9 of all JS/CSS in `web/dist/assets` (lazy chunks and Cuckoo Control included) |

The baseline lives in `tools/company-simulation/ui-performance-baseline.json` with its
thresholds, environment (Chromium version, CPU count, throttle, commit) and fixture sizes.
A metric regresses when it is more than **20 %** worse **and** worse by more than an
absolute floor: **50 ms** for timings, **2** long tasks, **5 kB** for bundles. Both
conditions keep jitter on fast interactions from raising warnings. Each run's numbers,
samples and the comparison are saved as `ui-performance-<time>.json` in the run directory.

Update the baseline only from a comparable machine and build, after confirming a change is
intended, and commit it with the reason. `--cpu-throttle 4` approximates a mid-range
phone CPU; the comparison notes when the baseline used a different throttle.

These are local debug-server measurements with scripted agents. They detect UI
regressions between commits on the same machine; they are not production latency
promises and do not replace React Profiler work on a reported slow interaction.

## First results, 2026-11-14

Commit `8ba5d31`, debug server, production client build, Chromium 153 headless, 4-core
Xeon container, seeded by one `run` rehearsal (12/12 scenarios passed). Nothing in the
application was changed in response; these are the findings to prioritise.

### Layout: 234/234 screen checks failed, 1,756 findings

Every screen opened, with no page errors. **No screen scrolls horizontally and nothing
extends past the right edge** at any size or theme. Every check still fails, mainly
because the account menu button is 2 px short on every screen. Light and dark gave
identical findings (878 each).

Layout breakage, highest priority:

| Screen | Sizes | Rule | Finding |
| --- | --- | --- | --- |
| Channel, agent direct message | 320×568 | `touch-target` | The composer textarea is squeezed to 22×160 px; the "Message" placeholder wraps one letter per line |
| Thread drawer (before schema 115) | 320×568 | `touch-target` | The "Reply in thread" box is 40×56 px between the composer buttons |
| Channel, agent direct message | 320×568, 360×740, 375×667 | `clipped-text` | The message list (`div[role=log]`, 250–305 px) clips its content to 306–313 px: the message actions toolbar reaches 369–376 px, so after a tap its last actions (⋯ More actions) are cut off |
| Channel, agent direct message | 320×568 | screenshot only | The conversation header has no room for the name: the channel name disappears and the agent's name shows one letter, because the rail, "‹ Chats" and three header buttons take the width (ellipsis is allowed by the rules, so this is not counted) |

Touch targets under 44×44 px (all nine sizes unless stated):

| Screen | Element (accessible name) | Size |
| --- | --- | --- |
| Every screen | Account menu button ("Your account") | 42×44 |
| Chat list (and beside the conversation on tablets) | Conversation rows (`a.chat-contact`) | full width × 40 |
| Chat list | Desktop alerts prompt "Turn on" / "Not now" | 70×30, 74×30 |
| Channel, agent DM, thread | Composer "Attach file", "Add emoji" | 40×40 |
| Channel, agent DM, thread | Composer textarea at 360 px and wider | 34 px tall |
| Channel, agent DM, thread, agents | Header icon buttons: Search, Notifications, thread Close, agent "More actions" | 36×44 |
| Agent DM | Header identity / message face / author name buttons ("View profile of …") | 32×32 (40×40 on tablets), 36×36, 20–40 px tall |
| Agent profile (Agents on tablets) | "Change who can talk to it", "Change managers", "Change model", "View tools" (360 px and wider) | 60×32, 43×32 |
| Agent profile | "Tools" tab (360 px and wider) | 37×48 |
| Activity | Tabs "Needs me", "Now", "Upcoming", "Tasks", "History", "Review"; "Search activity" | 38 px tall; 39 px tall |
| Apps | Filters "All", "Built here", "Your systems", "Services"; "Search apps" | 36 px tall; 40 px tall |
| Library | View buttons "All items", "Private", "Shared", "Starred"…; "Search Library" | 42 px tall |
| Settings, Profile, People | Settings page list buttons; "‹ Settings" (phones) | full width × 40; 82×40 |
| Settings → People | "Actions for <person>" | 36×44 |
| Channel (tablets only) | Header "People and agents in …" faces | 90×30 |
| Channel (820×1180) | Thread count "1 reply" | 380×18 |
| Home (820×1180) | "Open source conversation" link | 199×20 |

The full list, with every element's box, is in the run's `mobile-layout-*/report.json`
next to outlined screenshots.

### Speed baseline

Committed in `tools/company-simulation/ui-performance-baseline.json` (median of five
passes, no CPU throttle). A second `--strict` run against it straight afterwards stayed
inside the thresholds; its values are shown for scale.

| Metric | Baseline | Repeat run | Five baseline samples |
| --- | --- | --- | --- |
| Load Chat until the list shows | 399 ms | 381 ms | 297–498 ms |
| Home → Chat in the app | 110 ms | 87 ms | 93–147 ms |
| Switch 40-message channels | 109 ms | 98 ms | 79–155 ms (ten switches) |
| Open the 200-message channel | 362 ms | 364 ms | 219–464 ms; all 200 messages render at once |
| Long tasks: load / Home → Chat / switch / 200 messages | 0 / 0 / 0 / 1 | 0 / 0 / 0 / 1 | switch: 0–1 per switch |
| Start-up JS/CSS (gzip) | 271.7 kB | 271.7 kB | `main` chunk alone is 167 kB gzip |
| All JS/CSS assets (gzip) | 625.4 kB | 625.4 kB | |

Opening the 200-message channel renders all 200 messages, produces one long task and
takes roughly three times as long as a 40-message switch, consistent with the
unvirtualized transcript; the bounded 200-message history page caps it. The Vite build also warns that `main` exceeds
500 kB before compression.

### Running the checks in a root container

The first run was in a container whose shell runs as root, from a long worktree path.
The simulation needed: running `setup` and `serve` as an ordinary user (PostgreSQL
refuses root); a checkout path short enough for PostgreSQL's 107-byte Unix socket limit
(a short symlink to the checkout works, because the script uses the logical path);
`PG_BINDIR`/`PG_SHAREDIR` pointing at the system PostgreSQL 16; an `SSL_CERT_FILE` that
user can read; and `AYATI_CHROMIUM` set to Playwright's Chromium. A server that panics
before startup completes leaves `data/server.lock` behind, and the next start refuses the
directory as non-simulation data until that empty file is removed.
