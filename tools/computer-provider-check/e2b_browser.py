"""E2B check 2: Chromium with CDP in the computer; screenshot, screencast, input;
then sleep and wake and check the same browser, page and cookie. Never prints keys."""
import json, sys, time, pathlib
from e2b import Sandbox

results = {}
def step(name, fn):
    t = time.monotonic()
    try:
        value = fn()
        results[name] = {"ok": True, "seconds": round(time.monotonic() - t, 2), "value": value}
    except Exception as error:
        results[name] = {"ok": False, "seconds": round(time.monotonic() - t, 2), "error": f"{type(error).__name__}: {error}"[:500]}
    print(name, json.dumps(results[name], default=lambda o: type(o).__name__)[:500], flush=True)
    return results[name]

here = pathlib.Path(__file__).parent
box = None
try:
    created = step("create", lambda: Sandbox.create(timeout=900, metadata={"purpose": "cuckoo-provider-check"}))
    box = created["value"] if created["ok"] else None
    if box is None:
        sys.exit(1)
    run = lambda cmd, t=600: box.commands.run(cmd, timeout=t)
    step("install_chromium", lambda: run("sudo apt-get update -qq >/dev/null && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq chromium fonts-noto-core >/dev/null && chromium --version").stdout.strip())
    step("node_ws_and_overcommit", lambda: run("cd /home/user && npm install --silent ws >/dev/null 2>&1; sudo sysctl -qw vm.overcommit_memory=1; node --version").stdout.strip())
    step("disk_used", lambda: run("df -h / | tail -1").stdout.strip())
    box.files.write("/home/user/inside_cdp.mjs", (here / "inside_cdp.mjs").read_text())
    step("start_chromium", lambda: run("nohup chromium --headless=new --no-sandbox --remote-debugging-port=9222 --remote-allow-origins=* --disable-dev-shm-usage --no-zygote --disable-gpu --user-data-dir=/home/user/browser-profile --window-size=1280,800 about:blank >/tmp/chromium.log 2>&1 & sleep 3; curl -s http://127.0.0.1:9222/json/version | head -c 200").stdout)
    step("cdp_open_shot_cast_input", lambda: json.loads(run("cd /home/user && node inside_cdp.mjs open", 60).stdout))
    step("chromium_memory_mb", lambda: run("ps -C chromium -o rss= | awk '{s+=$1} END {print int(s/1024)}'").stdout.strip())
    step("save_screenshot", lambda: (here / "e2b-cdp-screenshot.png").write_bytes(bytes(box.files.read("/tmp/shot.png", format="bytes"))))
    sid = box.sandbox_id
    step("pause", lambda: box.pause())
    time.sleep(30)
    resumed = step("resume", lambda: Sandbox.connect(sid, timeout=900))
    if resumed["ok"]:
        box = resumed["value"]
        run = lambda cmd, t=600: box.commands.run(cmd, timeout=t)
        step("same_browser_after_wake", lambda: json.loads(run("cd /home/user && node inside_cdp.mjs check", 60).stdout))
        step("restart_browser_keeps_cookie", lambda: run("cd /home/user && node inside_cdp.mjs close; sleep 2; nohup chromium --headless=new --no-sandbox --remote-debugging-port=9222 --remote-allow-origins=* --disable-dev-shm-usage --no-zygote --disable-gpu --user-data-dir=/home/user/browser-profile https://example.com >/tmp/chromium.log 2>&1 & sleep 4; cd /home/user && node inside_cdp.mjs check", 60).stdout.strip())
finally:
    if box is not None:
        step("kill", lambda: box.kill())
print("SUMMARY", json.dumps({k: (v["ok"], v["seconds"]) for k, v in results.items()}))
