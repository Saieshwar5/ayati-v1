# Your own computers

Schema 119 (B3 of the agent building blocks, D248). A workspace can let chosen
agents work on a machine it runs itself, such as the office PC with the accounts
software, instead of on one of Cuckoo's [agent computers](agent-computers.md).
The machine runs the **Cuckoo Connector**
([`tools/cuckoo-connector/cuckoo_connector.py`](../tools/cuckoo-connector/cuckoo_connector.py)),
which connects out to Cuckoo and asks for work, so no ports are opened.

## How an admin sets it up

1. **Settings › Computers › Your computers**: type a name (*Office PC*) and press
   **Add computer**. Cuckoo shows a one-time code and the command to run, once.
   The code works once, for 15 minutes.
2. On the machine, with Python 3.9+ and bash (Git for Windows on Windows), run
   the command shown:

   ```bash
   python3 cuckoo_connector.py --server https://cuckoo.example.com --computer <id> --code ABCD-EFGH-JKLM-NPQR
   ```

   The connector swaps the code for a lasting key, kept in
   `~/.cuckoo-connector/<id>.key` (readable by that account only). Later starts
   need only `--server` and `--computer`.
3. **Choose agents**: an agent uses one computer at most, so choosing it here
   moves it off any other. Its Environment tab then says which machine it works on.

**New code** replaces a lost key or a new machine: the old key stops at once.
**Remove** stops new work at once, abandons queued work and signs the connector
out. Files on the machine stay there. Only admins see these controls; members see
the list.

## What agents can do there

The same four things as on Cuckoo's computers: run a command, write a file, read a
file and list a folder, so `computer_run`, `computer_*_file`, skill scripts
(`run_skill_script`), the agent's **Programs** rule (allow, ask first or block)
and Activity steps are unchanged. Each agent works in its own folder,
`<folder>/agents/<agent id>` (default folder `~/cuckoo-work`), which takes the
place of `/home/user`: paths are mapped into it, `HOME` points to it, and
`/home/user` in a command is rewritten to it. Paths outside the agent's folder,
including through links, are refused.

Not there yet: the **browser** and its live view stay on Cuckoo's computers. An
agent on a workspace computer is told plainly that browsing isn't available.

## How work moves

- Cuckoo queues each piece of work in `relay_workspace_computer_calls`. The
  connector long-polls `POST /hooks/computers/{id}/next` (about 25 seconds), does
  the work and answers `POST /hooks/computers/{id}/done`. Sign-in is
  `POST /hooks/computers/{id}/sign-in`. These three are the only routes under
  `/hooks/computers` and are exempt from the browser's Origin check; the code or
  `Authorization: Bearer <key>` authorizes them, no cookie does. Keys and codes
  are stored only as digests.
- **One request at a time.** A request holds the machine until it finishes (or
  three minutes after its last use). Another request waits up to 30 seconds, then
  is told the machine is busy.
- **Online** means heard from in the last 90 seconds. An agent is told plainly
  when its machine is waiting for its connector, offline or removed.
- **Nothing is retried.** Work not answered within its time limit plus 30 seconds
  is abandoned; a late answer is ignored. The agent is told the command *may have
  run* and to check before running it again.
- The connector logs only the kind of work and its id, never commands, output or
  file contents. Output is kept to 100 KB per stream, files to 1 MB.

## Safety

Agents run commands **as the account that runs the connector**. Use an account
that can reach only what agents need, keep backups, and use the Programs rule's
*Ask first* where a person should approve each command. Cuckoo's own computer
limits (sizes, internet, awake count, the operator's pause) don't apply to a
machine you run; the workspace's network and the account's permissions do.

## Checks

- Postgres (`shared_computers/tests.rs`): admin-only add, one-time code shown
  once and refused on reuse, choose agents, commands and files through the relay
  (base64 file content), the browser refused, busy and offline messages, remove
  refuses the connector, unanswered work abandoned with the late answer ignored,
  and a new code stopping the old key.
- Routes: only the three connector hooks skip the Origin check.
- Web (`web/tests/workspace-computers.test.mjs`): one add for a double click, the
  code dialog, choosing agents, removal failure keeping the row, members read-only.
- Connector (`python3 -m unittest discover -s tools/cuckoo-connector`): path
  confinement including links, the list format, commands with `HOME` mapped,
  timeouts, and sign-in → next → done against a stand-in server with the key file
  at 0600.
