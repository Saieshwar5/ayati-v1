"""E2B overnight check. `start`: make a computer with a file and a running counter, then
sleep it and print its id. `check ID`: wake it, report file and counter, then delete it."""
import json, sys, time
from e2b import Sandbox

if sys.argv[1] == "start":
    box = Sandbox.create(timeout=600, metadata={"purpose": "cuckoo-overnight"})
    box.files.write("/home/user/requests/r1/note.txt", f"written {time.strftime('%Y-%m-%d %H:%M:%S')}")
    box.commands.run("nohup sh -c 'i=0; while true; do i=$((i+1)); echo $i > /tmp/counter; sleep 1; done' >/dev/null 2>&1 &", background=True)
    time.sleep(3)
    before = box.commands.run("cat /tmp/counter").stdout.strip()
    box.pause()
    print(json.dumps({"id": box.sandbox_id, "counter_before": before, "paused_at": time.strftime("%Y-%m-%d %H:%M:%S")}))
else:
    started = time.monotonic()
    box = Sandbox.connect(sys.argv[2], timeout=600)
    woke = round(time.monotonic() - started, 2)
    out = {"woke_seconds": woke, "file": box.files.read("/home/user/requests/r1/note.txt"),
           "counter_now": box.commands.run("sleep 2; cat /tmp/counter").stdout.strip()}
    box.kill()
    out["deleted"] = True
    print(json.dumps(out))
