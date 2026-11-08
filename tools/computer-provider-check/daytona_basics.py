"""Daytona check 1: create, run, write, pause (memory), resume, stop (disk only), start,
domain allow list, delete. Reads DAYTONA_API_KEY from the environment; never prints it."""
import json, sys, time
import importlib, os
# The generated API clients ignore HTTPS_PROXY; route them through this session's proxy (this check only; Cuckoo calls the REST API directly).
for module in ["daytona_api_client", "daytona_toolbox_api_client", "daytona_analytics_api_client"]:
    cls = importlib.import_module(module).Configuration
    original = cls.__init__
    def init(self, *args, _original=original, **kwargs):
        _original(self, *args, **kwargs)
        self.proxy = os.environ.get("HTTPS_PROXY")
        self.ssl_ca_cert = "/root/.ccr/ca-bundle.crt"
    cls.__init__ = init
from daytona import Daytona, CreateSandboxFromSnapshotParams

results = {}
def step(name, fn):
    t = time.monotonic()
    try:
        value = fn()
        results[name] = {"ok": True, "seconds": round(time.monotonic() - t, 2), "value": value}
    except Exception as error:
        results[name] = {"ok": False, "seconds": round(time.monotonic() - t, 2), "error": f"{type(error).__name__}: {error}"[:400]}
    print(name, json.dumps(results[name], default=lambda o: type(o).__name__)[:300], flush=True)
    return results[name]

client = Daytona()
box = None
run = lambda cmd: box.process.exec(cmd, timeout=60).result
try:
    created = step("create", lambda: client.create(CreateSandboxFromSnapshotParams(
        labels={"purpose": "cuckoo-provider-check"}, auto_stop_interval=15, ephemeral=False,
        ), timeout=120))
    box = created.get("value") if created["ok"] else None
    if box is None:
        sys.exit(1)
    step("command", lambda: run("uname -a; head -2 /etc/os-release; nproc; free -m | head -2; df -h / | tail -1"))
    step("write_file", lambda: box.fs.upload_file(b"kept across sleep", "requests/r1/note.txt") or "written")
    step("start_background", lambda: run("nohup sh -c 'i=0; while true; do i=$((i+1)); echo $i > /tmp/counter; sleep 1; done' >/dev/null 2>&1 & echo started"))
    time.sleep(3)
    step("allowed_site", lambda: run("for u in https://example.com https://pypi.org https://github.com https://www.google.com; do printf \"%s \" $u; curl -s -o /dev/null -w \"%{http_code}\\\\n\" --max-time 10 $u; done"))
    step("blocked_site", lambda: run("curl -s -o /dev/null -w '%{http_code}' --max-time 10 https://www.google.com || echo blocked"))
    step("counter_before_pause", lambda: run("cat /tmp/counter").strip())
    step("pause", lambda: box.pause(timeout=120))
    time.sleep(15)
    step("resume_after_pause", lambda: box.start(timeout=120))
    step("file_after_pause", lambda: box.fs.download_file("requests/r1/note.txt").decode())
    step("counter_after_pause", lambda: run("sleep 2; cat /tmp/counter 2>/dev/null || echo gone").strip())
    step("stop", lambda: box.stop(timeout=120))
    step("start_after_stop", lambda: box.start(timeout=120))
    step("file_after_stop", lambda: box.fs.download_file("requests/r1/note.txt").decode())
    step("counter_after_stop", lambda: run("sleep 2; cat /tmp/counter 2>/dev/null || echo gone").strip())
finally:
    if box is not None:
        step("delete", lambda: box.delete(timeout=60, wait=True))
print("SUMMARY", json.dumps({k: (v["ok"], v["seconds"]) for k, v in results.items()}))
