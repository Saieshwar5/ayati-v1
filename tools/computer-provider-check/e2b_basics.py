"""E2B check 1: create, run, write, pause (memory), resume, network allow list, kill.
Reads E2B_API_KEY from the environment; never prints it."""
import json, sys, time
from e2b import Sandbox

results = {}
def step(name, fn):
    t = time.monotonic()
    try:
        value = fn()
        results[name] = {"ok": True, "seconds": round(time.monotonic() - t, 2), "value": value}
    except Exception as error:  # report and continue so cleanup still runs
        results[name] = {"ok": False, "seconds": round(time.monotonic() - t, 2), "error": f"{type(error).__name__}: {error}"[:400]}
    print(name, json.dumps(results[name], default=lambda o: type(o).__name__)[:300], flush=True)
    return results[name]

box = None
try:
    created = step("create", lambda: Sandbox.create(timeout=600, metadata={"purpose": "cuckoo-provider-check"},
        network={"allow_out": ["example.com", "pypi.org", "files.pythonhosted.org"], "deny_out": ["0.0.0.0/0"]}))
    box = created.get("value") if created["ok"] else None
    if box is None:
        sys.exit(1)
    results["create"]["value"] = box.sandbox_id
    step("command", lambda: box.commands.run("uname -a && cat /etc/os-release | head -2 && nproc && free -m | head -2 && df -h / | tail -1").stdout)
    step("write_file", lambda: box.files.write("/home/user/requests/r1/note.txt", "kept across sleep") and "written")
    step("start_background", lambda: box.commands.run("nohup sh -c 'i=0; while true; do i=$((i+1)); echo $i > /tmp/counter; sleep 1; done' >/dev/null 2>&1 &", background=True) and "started")
    time.sleep(3)
    step("allowed_site", lambda: box.commands.run("curl -s -o /dev/null -w '%{http_code}' --max-time 10 https://example.com").stdout)
    def blocked():
        r = box.commands.run("curl -s -o /dev/null -w '%{http_code}' --max-time 10 https://www.google.com || echo blocked", )
        return r.stdout
    step("blocked_site", blocked)
    before = step("counter_before_pause", lambda: box.commands.run("cat /tmp/counter").stdout.strip())
    sandbox_id = box.sandbox_id
    step("pause_full", lambda: box.pause())
    time.sleep(20)
    resumed = step("resume", lambda: Sandbox.connect(sandbox_id, timeout=600))
    if resumed["ok"]:
        box = resumed["value"]
        results["resume"]["value"] = "connected"
        step("file_after_resume", lambda: box.files.read("/home/user/requests/r1/note.txt"))
        step("counter_after_resume", lambda: box.commands.run("sleep 2; cat /tmp/counter").stdout.strip())
        step("info", lambda: str(box.get_info())[:300])
finally:
    if box is not None:
        step("kill", lambda: box.kill())
print("SUMMARY", json.dumps({k: (v["ok"], v["seconds"]) for k, v in results.items()}))
