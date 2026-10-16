# Local agent browser contract

This increment uses upstream agent-browser 0.39.1 (Apache-2.0) and Chrome for
Testing 155.0.8059.84 inside a real Ubuntu guest. Exact download URLs, SHA-256
digests and Ubuntu library versions are in browser-release.json. Chrome, its
credits and the Ubuntu package copyright files remain in the local bundle.
Review their distribution terms before distributing that bundle. No browser
binary or private profile is tracked in Git.

The Ubuntu shared-library pins are in browser-libraries.json.

## Setup and supported scope

Run the existing Goose/VM prerequisites first, then:

```sh
/usr/bin/python3 tools/install-browser.py
RUSTFLAGS='-C target-feature=+crt-static' cargo build --locked -p ayati-guest --target x86_64-unknown-linux-gnu --release -j 2
/usr/bin/python3 tools/prepare-local-vm.py --directory .ayati-local/vms/browser-agent --browser
```

Use a new control data directory when changing the saved runtime/tool settings:

```dotenv
AYATI_EXECUTION=vm
AYATI_BROWSER=1
AYATI_VM_MANIFEST=.ayati-local/vms/browser-agent/vm.json
AYATI_DATA_DIR=.ayati-local/browser-control
```

The optional browser VM has one vCPU and 1536 MiB RAM. Existing 768 MiB guests
and their disks are preserved. Browser execution has no host fallback. The
installer downloads into the project cache, validates every artifact, and uses
safe extraction. It requires bsdtar and Python's standard library. Nothing is
installed into the host OS or its services.

This remains one local owner/agent. The daemon is loopback-only; existing
Host/Origin/fetch-site guards cover the browser endpoints. It is not production
authentication or multi-business authorization. `AYATI_BROWSER_FIXTURE_PORT`
is a test-only opt-in mapping of fixture.ayati.invalid to a loopback synthetic
HTTP server; leave it unset in ordinary use.

## Tools, profiles and egress

Goose sees only Ayati's small stdio MCP surface: HTTP(S) navigation, compact
snapshots, reference clicks/fill, key presses, scroll, bounded waits, basic tabs,
viewport screenshots and bounded file transfers. Every call uses the existing
exact Allow once / Deny approval. Unknown operations, options, arbitrary selectors,
eval, cookie/state export, debugging endpoints and profile/configuration commands
are refused. The trusted broker alone uses upstream's private MCP connection;
upstream's broad tool profiles are never exposed directly to Goose.

The browser broker runs as a separate unprivileged guest account. Its durable
profile, upstream sockets, temporary captures and authentication files are private
to that account. Goose has no general shell/file extension. Profiles are not
copied into memory tools, ordinary workspace files or model prompts. Browser
authentication is separate from future business connector grants; cross-path
revocation is not implemented.

QEMU keeps restricted networking. Browser HTTP(S) uses a host proxy that validates
DNS answers and pins the actual connection to a public IPv4 address, with only
ports 80/443. Loopback, private, link-local, metadata, multicast, reserved ranges
and IPv6 are refused. Redirects require another validated proxy destination.
Raw CDP/stream ports have no host forward. The model gateway keeps its separate
token and never supplies the real provider key to Chrome. This is network
containment, not financial/operation authorization for arbitrary websites.

The guest AppArmor profile permits Chrome's user namespace sandbox at its pinned
path. Ayati does not request --no-sandbox. Guest/Chrome vulnerabilities and full
production hypervisor security are outside this local acceptance proof.

Element actions require a snapshot in the current run and after every mutation.
New runs and owner handback invalidate references. agent-browser still owns DOM
reference resolution; arbitrary dynamic-site success is not established. New
tab commands refuse more than four tabs; website-created popups also need care.

## Owner view and takeover

The Browser button opens a view of the guest's actual browser. This first local
viewer polls bounded JPEG frames once per second through the authenticated SSH
bridge and loopback API. It is an interactive view with mapped mouse/touch,
scroll and keyboard/text input. It does not expose upstream WebSocket/CDP ports.
High-frame-rate streaming and remote mobile access are later viewer work.

Take control persists a controller capability outside Goose before waking or
transferring the browser. The guest serializes commands, drains prior work, saves
owner mode and then acknowledges. A different controller capability cannot send
input or read owner frames. Takeover interrupts any active Goose turn, invalidates
pending approvals and blocks queued work. Interrupted work needs deliberate follow-up.

While the owner controls the browser, all agent observations and actions are
blocked. Owner input requests/frames bypass conversation/action logs and provider
requests. After explicit handback, ordinary visible page content can be read again;
password/OTP inputs are cleared first. Captures use a private temporary file that is removed after the reply;
HTTP responses use no-store. The browser's own profile/site state persists.
Use owner mode for credentials; never send them in chat. An agent fill is refused
on pages containing a password/one-time-code field.

Closing the panel, disconnecting, restarting Ayati or losing a reply does not
hand control back. The same tab retains its capability in sessionStorage; if
storage is unavailable it lasts only for that page lifetime. No automatic lease
expiry resumes the agent. Keep the controlling tab available. Recovery after
losing the tab entirely is available locally with `cargo run -- browser release`
using the daemon's same configuration/data directory. It reads the private saved
capability without printing it, reclaims the browser and explicitly hands it back.
This is a local operator action, not public account recovery.

The local developer API is `GET /api/browser/status` and
`POST /api/browser/control`. Requests contain an `op` and, for owner operations,
the controlling tab's UUID `token`:

| Operation | Additional fields |
| --- | --- |
| take / resume | token |
| finish | token; explicitly close the browser and release unfinished interaction |
| frame | token while owner control is active; returns MCP image content |
| input | token, tool (allowed upstream input name), arguments |
| put_file | token, file (basename), data (base64, at most 2 MiB decoded) |
| get_file | token, file; returns base64 data |

The API does not expose arm/disarm or direct agent execution to web clients.

Hand back clears password/OTP inputs, obtains a fresh observation, revokes owner
input and releases the host's saved pause. The next run must observe again.
Pending approvals do not survive takeover. Separate run identities prevent an
old MCP process from acting after a new run is armed. General Stop remains final
for its job; VM Stop cancels queued work and retains the existing dispatch pause.

Owner viewing/requests hold a VM activity lease; persistent owner mode also blocks
idle release and queue dispatch after restart. Once ownership is released, the
normal idle timeout can stop the guest and preserve its browser profile. Before
power-down the host explicitly parks the broker and waits for upstream Browser.close
to flush the profile. Parked brokers reject late viewer requests instead of
reopening Chrome. Systemd uses mixed termination as a fallback. Forced termination
does not establish that the last profile changes were flushed.

Form edits, clicks, key input and pending transfers conservatively retain an
interaction lease after a turn ends, including across tab changes. They prevent
automatic VM shutdown until the owner selects **Close browser session** or approves
the MCP close tool. Closing only the viewer panel or handing control back does
not discard this live state. Read-only browser work still releases automatically.
Explicit VM Stop can discard unfinished interaction. The durable owner capability
is Ayati-owned browser-owner.json beside the host database, outside Goose state.

## Transfers and uncertain effects

Transfers use only validated basenames in a dedicated private directory. The
owner puts/gets files through control API operations; agent upload/download tools
cannot select profile, core, host or arbitrary filesystem paths. Transfers have
a 2 MiB per-file limit and an 8 MiB tmpfs capacity; they disappear after VM shutdown.
Download failure does not imply an external action did not happen. Existing
filenames cannot be silently overwritten/retried.

All mutating calls are sent once. Upstream's outcome_unknown and transport loss
remain uncertain, prevent a false completed job and require inspection. A poisoned
broker refuses further automation/takeover until the VM is stopped/restarted;
new runs require fresh observations. Restoring a profile never reverses an
external write. Important business actions still need trusted connectors or
owner completion; browser approval alone cannot enforce business financial limits.

## Checks

```sh
AYATI_VM_TEST_MANIFEST=/absolute/path/to/disposable-browser/vm.json node --test --test-concurrency=1 tests/browser_vm.mjs
```

Use a disposable browser-enabled VM: this changes its profile and test-site state.
The suite uses real Goose, MCP, Chrome and KVM with a scripted local model/site;
no paid inference or live account action is implied. Record actual passed checks
and material gaps in the current implementation status; setup alone is not proof.
