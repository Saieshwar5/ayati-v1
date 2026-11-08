# Agent computer providers: real check (2026-11-08)

The first step of [agent computers](../designs/agent-computers/README.md) was a real
check of the providers with the owner's keys, before any product code. The scripts
are in [tools/computer-provider-check](../tools/computer-provider-check/README.md).
Tested with the `e2b` SDK 2.63.0, the `daytona` SDK 0.231.0 and Chromium 156. Keys
were read from the environment and are not stored in the repository.

## Results

### E2B: everything the design needs works

| Check | Result |
| --- | --- |
| Create a computer | 0.3–0.7 s; Debian 12, 2 CPUs, 473 MB memory by default, 22 GB disk |
| Run a command; write and read a file | 0.5 s; 0.1 s |
| Website allow list (`allow_out` domains, everything else denied) | Listed website 200; any other website blocked |
| Sleep keeping memory (`pause`) / wake (`connect`) | 0.2–1.3 s / 0.2–0.4 s |
| After waking | Files kept; a running program kept counting; the browser was on the same page with the same cookie and even half-typed text |
| Chromium with CDP | Page opened, screenshot (61 KB), page read as text |
| Live picture (`Page.startScreencast`, JPEG quality 60, 1280×800) | 10 frames a second, about 42 KB a frame, sent only when the page changes |
| A person's click and typing (`Input.dispatchMouseEvent`, `Input.insertText`) | Landed in the page field |
| Browser closed properly (`Browser.close`), then restarted | Login cookie kept |
| Overnight sleep | Started 11:14 UTC; result to be added on 2026-11-09 |

### Daytona: not usable for agent computers on this account yet

| Check | Result |
| --- | --- |
| Create, command, file | 0.4 s, 0.35 s, 0.3 s; Debian 13 |
| Isolation | A container sharing the host's kernel (it reports the host's 96 CPUs), not a VM |
| VM computers (`kvm`) | "KVM sandboxes are not enabled for this organization" |
| Sleep keeping memory (`pause`) | "Pausing is not supported for this sandbox" |
| Stop and start | 1.6 s and 1.9 s; files kept, running programs stopped |
| Website allow list per computer | Refused: "Network access is restricted and cannot be overridden at the sandbox level" (tier based) |
| Websites reachable | pypi.org only; example.com, github.com and google.com not reachable |

## What this changes in the build

1. **E2B is the first provider.** Daytona waits until the account has VM computers
   and a network tier that allows per-computer website lists; that needs a request
   to Daytona. Containers that share a kernel are not enough isolation for company
   data. AWS Lambda MicroVMs come later, as decided.
2. **Cuckoo's image must contain the browser.** Installing Chromium at wake takes
   24–38 s. The image (an E2B template, built from Cuckoo's own Dockerfile) bakes in
   Chromium, Node, Python, document tools and fonts for Indian scripts.
3. **Sizes start at 2 GB.** Chromium alone used 330–520 MB on the 473 MB default,
   so Cuckoo's Small size is 1 CPU and 2 GB. Memory is set when the image is built.
4. **Chromium settings that work:** `--headless=new --no-sandbox --disable-dev-shm-usage
   --no-zygote --disable-gpu --remote-debugging-port=9222` on 127.0.0.1 only, and
   the computer's `vm.overcommit_memory=1`. Without the latter, Chromium's page
   process fails ("V8 process OOM: failed to reserve virtual memory for CodeRange")
   and CDP calls to the page hang. The debugging port is never exposed outside
   the computer: Cuckoo's helper inside the computer talks to it.
5. **Close the browser properly before saving the disk.** Chromium writes cookies
   to disk only from time to time. Cuckoo closes it with `Browser.close` before
   copying the disk or before a sleep that keeps only the disk; otherwise a new
   login can be lost.
6. **The live picture is cheap enough.** At 10 frames a second it is about 3.4
   Mbit/s while the page changes and nothing while it is still. Phones get a
   smaller picture (about 720 px wide, lower quality and frame rate).
7. **Network for Cuckoo itself:** the controller needs only `api.e2b.app` (control)
   and `sandbox.e2b.app` (commands, files and the helper). The computers' own
   internet is controlled by E2B's `allow_out` and `deny_out` rules, and can be
   changed while a computer runs (`update_network`): the second lock of "browser
   off" in the design.
8. **Sleep is built in.** E2B can pause a computer on its own timeout and wake it
   on the next request (`lifecycle: on_timeout pause, auto_resume`). Cuckoo still
   decides when to sleep (idle time, request end, budgets), using this as a safety
   net.
9. **Daytona's Python SDK ignores the HTTPS proxy setting** (a generated client).
   This affected only the check scripts: Cuckoo's controller calls the providers'
   REST APIs from Rust.

## Not yet checked

- Overnight and multi-day sleep (overnight result due 2026-11-09).
- Building a custom E2B template with the browser baked in, and its memory sizes.
- Real costs per wake from E2B's billing, against the price assumptions.
- AWS Lambda MicroVMs (later, as decided).
