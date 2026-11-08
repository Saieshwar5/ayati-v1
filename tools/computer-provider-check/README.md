# Computer provider check

Real checks of the computer providers behind [agent computers](../../runtime/agent-computer-providers.md).
They create real computers and cost a few cents. Every script deletes what it made,
even when a step fails. Results are recorded in that runtime document.

```bash
python3 -m venv .venv && .venv/bin/pip install e2b daytona
```

Keys come from the environment and are never printed or saved:
`E2B_API_KEY` and `DAYTONA_API_KEY`. The network must allow `api.e2b.app`,
`sandbox.e2b.app`, `app.daytona.io` and `proxy.app.daytona.io`.

| Script | Checks |
| --- | --- |
| `e2b_basics.py` | Create, command, file, website allow list, sleep with memory, wake, file and running program kept |
| `e2b_browser.py` | Chromium with CDP: page, screenshot, screencast, a person's click and typing, page text; sleep and wake on the same page; a properly closed browser keeps its cookies (uses `inside_cdp.mjs` inside the computer) |
| `e2b_overnight.py` | `start` sleeps a computer with a file and a running counter; `check ID` a day later wakes it, reports and deletes it |
| `daytona_basics.py` | The same lifecycle on Daytona: create, command, file, pause, stop and start, websites reachable |

Run with the venv's Python in isolated mode, for example `.venv/bin/python -I e2b_basics.py`.
If a run is interrupted, delete leftovers: they carry the label `purpose=cuckoo-provider-check`.
