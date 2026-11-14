#!/usr/bin/env python3
"""Cuckoo Connector: lets chosen Cuckoo agents work on a computer you run.

An admin adds the computer in Settings > Computers > Your computers and gets
a one-time code. Run this on that computer with the command shown there:

    python3 cuckoo_connector.py --server https://cuckoo.example.com \\
        --computer <id> --code ABCD-EFGH-JKLM-NPQR

The code is swapped for a lasting key, kept in a file only this account can
read. Later starts need only --server and --computer. The connector connects
out to Cuckoo and asks for work, so no ports are opened.

Each agent works in its own folder, <folder>/agents/<agent id>, where Cuckoo's
computers use /home/user. Commands run as this account, with that folder as
HOME. They are bash commands, so on Windows install Git for Windows (bash).
Files outside the agent's folder are refused. The connector logs only the
kind of work and its id, never commands, output or file contents.

Uses only the Python 3.9+ standard library. See runtime/workspace-computers.md.
"""
import argparse
import base64
import json
import os
import platform
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

# Where Cuckoo's own computers keep an agent's files; mapped to its folder here.
CUCKOO_HOME = "/home/user"
# Output kept from each stream of a command, like Cuckoo's computers.
KEPT_BYTES = 100_000
# Largest file read or written through the connector.
FILE_BYTES = 1_000_000
LIST_LIMIT = 200


class Refused(Exception):
    """Cuckoo refused the key or code: sign in again with a new code."""


def log(message):
    print(time.strftime("%H:%M:%S"), message, flush=True)


def post(server, path, body, key=None, timeout=40):
    request = urllib.request.Request(
        server.rstrip("/") + path,
        data=json.dumps(body).encode(),
        method="POST",
        headers={"Content-Type": "application/json", "User-Agent": "cuckoo-connector/1"},
    )
    if key:
        request.add_header("Authorization", "Bearer " + key)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return json.loads(response.read() or b"{}")
    except urllib.error.HTTPError as error:
        if error.code == 401:
            raise Refused() from None
        raise


def key_file(computer, given):
    if given:
        return Path(given).expanduser()
    return Path.home() / ".cuckoo-connector" / (computer + ".key")


def sign_in(server, computer, code, keys):
    answer = post(server, f"/hooks/computers/{computer}/sign-in", {
        "code": code, "platform": f"{platform.system()} {platform.release()}".strip(),
    })
    keys.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    # Created readable by this account only, before the key is written.
    handle = os.open(keys, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(handle, "w") as file:
        file.write(answer["key"])
    log("Signed in. The key is kept in " + str(keys))
    return answer["key"]


class Agent:
    """One agent's folder, and the paths and commands mapped into it."""

    def __init__(self, base, agent):
        uuid.UUID(agent)  # Cuckoo sends agent ids; anything else is refused.
        self.root = (base / "agents" / agent).resolve()
        self.root.mkdir(parents=True, exist_ok=True)

    def path(self, given):
        if given != CUCKOO_HOME and not given.startswith(CUCKOO_HOME + "/"):
            raise ValueError("Only paths in your own folder can be used here")
        mapped = (self.root / given[len(CUCKOO_HOME):].lstrip("/")).resolve()
        # resolve() follows links, so a link pointing outside is refused too.
        if mapped != self.root and self.root not in mapped.parents:
            raise ValueError("Only paths in your own folder can be used here")
        return mapped

    def command(self, text):
        return text.replace(CUCKOO_HOME, self.root.as_posix())


def kept(data):
    return data[:KEPT_BYTES].decode("utf-8", "replace")


def run(agent, call):
    cwd = agent.path(call.get("cwd") or CUCKOO_HOME)
    cwd.mkdir(parents=True, exist_ok=True)
    bash = shutil.which("bash")
    if not bash:
        raise ValueError("This computer has no bash, so commands can't run here")
    env = dict(os.environ, HOME=str(agent.root), CUCKOO_HOME=str(agent.root))
    seconds = max(1, min(int(call.get("seconds") or 120), 600))
    process = subprocess.Popen(
        [bash, "-c", agent.command(call["command"])], cwd=cwd, env=env,
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        start_new_session=os.name == "posix",
    )
    timed_out = False
    try:
        stdout, stderr = process.communicate(timeout=seconds)
    except subprocess.TimeoutExpired:
        timed_out = True
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        else:
            process.kill()
        stdout, stderr = process.communicate()
    return {
        "exit_code": 124 if timed_out else process.returncode,
        "stdout": kept(stdout), "stderr": kept(stderr),
        "output_bytes": len(stdout) + len(stderr), "timed_out": timed_out,
    }


def write(agent, call):
    data = base64.b64decode(call["content_b64"], validate=True)
    if len(data) > FILE_BYTES:
        raise ValueError("The file is too large")
    target = agent.path(call["path"])
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    return {}


def read(agent, call):
    source = agent.path(call["path"])
    if not source.is_file():
        raise ValueError("No such file")
    if source.stat().st_size > FILE_BYTES:
        raise ValueError("The file is too large")
    return {"content_b64": base64.b64encode(source.read_bytes()).decode()}


def listing(agent, call):
    """Lines of `type<TAB>bytes<TAB>changed<TAB>name`, as Cuckoo's computers list."""
    folder = agent.path(call["path"])
    folder.mkdir(parents=True, exist_ok=True)
    lines = []
    for entry in sorted(folder.iterdir(), key=lambda e: e.name)[:LIST_LIMIT]:
        info = entry.stat(follow_symlinks=False)
        kind = "d" if entry.is_dir() and not entry.is_symlink() else "f"
        changed = time.strftime("%Y-%m-%d %H:%M", time.localtime(info.st_mtime))
        lines.append(f"{kind}\t{info.st_size}\t{changed}\t{entry.name}")
    return {"stdout": "\n".join(lines)}


WORK = {"run": run, "write": write, "read": read, "list": listing}


def answer(base, call):
    try:
        work = WORK.get(call.get("kind"))
        if not work:
            raise ValueError("This connector can't do that yet; update it")
        return work(Agent(base, call["agent"]), call)
    except (ValueError, OSError, KeyError) as error:
        return {"error": str(error) or type(error).__name__}


def serve(server, computer, key, base):
    log(f"Waiting for work for computer {computer}")
    pause = 1
    while True:
        try:
            call = post(server, f"/hooks/computers/{computer}/next", {}, key)["call"]
            pause = 1
            if not call:
                continue
            log(f"{call['kind']} {call['id']}")
            result = answer(base, call)
            post(server, f"/hooks/computers/{computer}/done", {"call": call["id"], "result": result}, key)
        except Refused:
            raise
        except (OSError, ValueError, KeyError) as error:
            # Cuckoo or the network is down: wait and ask again. Work is never
            # repeated; Cuckoo abandons what it didn't hear back about.
            log(f"Can't reach Cuckoo ({type(error).__name__}); trying again in {pause} s")
            time.sleep(pause)
            pause = min(pause * 2, 60)


def main():
    parser = argparse.ArgumentParser(description="Let chosen Cuckoo agents work on this computer.")
    parser.add_argument("--server", required=True, help="Your Cuckoo address, such as https://cuckoo.example.com")
    parser.add_argument("--computer", required=True, help="The computer's id from Settings > Computers")
    parser.add_argument("--code", help="The one-time code, needed the first time only")
    parser.add_argument("--folder", default=str(Path.home() / "cuckoo-work"), help="Where agents' folders are kept")
    parser.add_argument("--key-file", help="Where the key is kept (default ~/.cuckoo-connector/<id>.key)")
    options = parser.parse_args()
    uuid.UUID(options.computer)
    if not options.server.startswith(("https://", "http://localhost:", "http://127.0.0.1:")):
        sys.exit("Use your Cuckoo's https:// address")
    base = Path(options.folder).expanduser().resolve()
    if any(c in base.as_posix() for c in " '\"\\$`"):
        sys.exit("Choose a --folder whose path has no spaces or quotes")
    keys = key_file(options.computer, options.key_file)
    try:
        if options.code:
            key = sign_in(options.server, options.computer, options.code, keys)
        elif keys.exists():
            key = keys.read_text().strip()
        else:
            sys.exit("Give the one-time code with --code the first time")
        serve(options.server, options.computer, key, base)
    except Refused:
        sys.exit("Cuckoo refused this connector. Ask an admin for a new code and start it with --code.")
    except KeyboardInterrupt:
        log("Stopped")


if __name__ == "__main__":
    main()
