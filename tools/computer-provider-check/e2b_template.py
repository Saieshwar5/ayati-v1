"""Build Cuckoo's agent computer image as an E2B template, then check a computer
started from it: browser ready with nothing to install, memory, and the CDP check.
Usage: e2b_template.py [alias] [--no-build]  (default cuckoo-computer-dev). Never prints keys."""
import json, pathlib, sys, time
from e2b import Sandbox, Template

here = pathlib.Path(__file__).parent
dockerfile = here.parent.parent / "deploy" / "agent-computer" / "Dockerfile"
args = [a for a in sys.argv[1:] if not a.startswith("--")]
alias = args[0] if args else "cuckoo-computer-dev"
build = "--no-build" not in sys.argv

# Chromium's page process needs memory overcommit; set once when the template's
# snapshot is made, so every computer started from it has it.
template = Template().from_dockerfile(dockerfile.read_text()).set_start_cmd(
    "sudo sysctl -w vm.overcommit_memory=1", "test \"$(cat /proc/sys/vm/overcommit_memory)\" = 1")
if build:
    started = time.monotonic()
    info = Template.build(template, alias=alias, cpu_count=1, memory_mb=2048,
                          on_build_logs=lambda entry: print("  build:", str(entry)[:160], flush=True))
    print(json.dumps({"built": alias, "seconds": round(time.monotonic() - started, 1), "info": str(info)[:200]}), flush=True)

box = None
try:
    t = time.monotonic()
    box = Sandbox.create(alias, timeout=600, metadata={"purpose": "cuckoo-provider-check"})
    created = round(time.monotonic() - t, 2)
    run = lambda c, s=120: box.commands.run(c, timeout=s).stdout.strip()
    report = {"create_seconds": created,
              "memory": run("free -m | sed -n 2p"),
              "overcommit": run("cat /proc/sys/vm/overcommit_memory"),
              "chromium": run("chromium --version"),
              "fonts_devanagari": run("fc-list :lang=hi family | head -1"),
              "pdftotext": run("pdftotext -v 2>&1 | head -1")}
    box.files.write("/home/user/inside_cdp.mjs", (here / "inside_cdp.mjs").read_text())
    t = time.monotonic()
    run("nohup chromium --headless=new --no-sandbox --disable-dev-shm-usage --no-zygote --disable-gpu "
        "--remote-debugging-port=9222 --remote-allow-origins=* --user-data-dir=/home/user/browser-profile "
        "about:blank >/tmp/chromium.log 2>&1 & for i in $(seq 1 50); do curl -s 127.0.0.1:9222/json/version >/dev/null && break; sleep 0.1; done; echo up")
    report["browser_ready_seconds"] = round(time.monotonic() - t, 2)
    report["cdp_check"] = json.loads(run("cd /home/user && ln -sfn /opt/cuckoo/node_modules node_modules && node inside_cdp.mjs open", 60))
    report["chromium_memory_mb"] = run("ps -C chromium -o rss= | awk '{s+=$1} END {print int(s/1024)}'")
    print(json.dumps(report, indent=1))
finally:
    if box is not None:
        box.kill()
