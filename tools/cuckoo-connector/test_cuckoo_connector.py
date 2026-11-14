"""Checks for the Cuckoo Connector, standard library only:

    python3 -m unittest discover -s tools/cuckoo-connector
"""
import base64
import json
import os
import tempfile
import threading
import unittest
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

import cuckoo_connector as connector

AGENT = str(uuid.uuid4())


class Work(unittest.TestCase):
    def setUp(self):
        self.base = Path(tempfile.mkdtemp())

    def do(self, **call):
        return connector.answer(self.base, {"agent": AGENT, **call})

    def test_files_round_trip_inside_the_agents_folder(self):
        text = base64.b64encode(b"Paid in full").decode()
        self.assertEqual(self.do(kind="write", path="/home/user/requests/r1/notes.txt", content_b64=text), {})
        self.assertTrue((self.base / "agents" / AGENT / "requests/r1/notes.txt").is_file())
        self.assertEqual(self.do(kind="read", path="/home/user/requests/r1/notes.txt"), {"content_b64": text})
        listed = self.do(kind="list", path="/home/user/requests/r1")["stdout"].split("\t")
        self.assertEqual((listed[0], listed[1], listed[3]), ("f", "12", "notes.txt"))

    def test_paths_outside_the_agents_folder_are_refused(self):
        outside = self.base / "secret.txt"
        outside.write_text("private")
        (self.base / "agents" / AGENT).mkdir(parents=True)
        os.symlink(outside, self.base / "agents" / AGENT / "link")
        for path in ["/etc/passwd", "/home/user/../../secret.txt", "/home/user/link", "/home/username/x", "relative.txt"]:
            self.assertIn("error", self.do(kind="read", path=path), path)
        self.assertIn("error", connector.answer(self.base, {"agent": "../x", "kind": "read", "path": "/home/user/a"}))
        self.assertIn("error", self.do(kind="format_disk"))

    def test_commands_run_in_the_agents_folder_with_home_mapped(self):
        self.do(kind="write", path="/home/user/in.txt", content_b64=base64.b64encode(b"from Cuckoo").decode())
        ran = self.do(kind="run", command="pwd; echo $HOME; cat '/home/user/in.txt'; echo oops >&2; exit 3",
                      cwd="/home/user/requests/r2", seconds=10)
        root = str((self.base / "agents" / AGENT).resolve())
        self.assertEqual(ran["exit_code"], 3)
        self.assertEqual(ran["stdout"].splitlines(), [root + "/requests/r2", root, "from Cuckoo"])
        self.assertEqual(ran["stderr"].strip().splitlines()[-1], "oops")
        self.assertFalse(ran["timed_out"])

    def test_a_command_past_its_limit_is_stopped(self):
        ran = self.do(kind="run", command="sleep 30", cwd="/home/user", seconds=1)
        self.assertTrue(ran["timed_out"])
        self.assertEqual(ran["exit_code"], 124)


class Server(BaseHTTPRequestHandler):
    """Plays Cuckoo's three connector hooks for one piece of work."""
    calls = []
    answers = []

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])) or b"{}")
        key = self.headers.get("Authorization")
        if self.path.endswith("/sign-in"):
            reply = {"key": "ckc_test"} if body["code"] == "ABCD" else None
        elif key != "Bearer ckc_test":
            reply = None
        elif self.path.endswith("/next"):
            reply = {"call": Server.calls.pop(0) if Server.calls else None}
        else:
            Server.answers.append(body)
            reply = {"saved": True}
        self.send_response(200 if reply is not None else 401)
        self.end_headers()
        self.wfile.write(json.dumps(reply or {}).encode())

    def log_message(self, *args):
        pass


class Loop(unittest.TestCase):
    def test_signs_in_once_keeps_the_key_private_and_answers_work(self):
        http = ThreadingHTTPServer(("127.0.0.1", 0), Server)
        threading.Thread(target=http.serve_forever, daemon=True).start()
        server = f"http://localhost:{http.server_port}"
        computer = str(uuid.uuid4())
        keys = Path(tempfile.mkdtemp()) / "keys" / "c.key"
        with self.assertRaises(connector.Refused):
            connector.sign_in(server, computer, "WRONG", keys)
        key = connector.sign_in(server, computer, "ABCD", keys)
        self.assertEqual(key, "ckc_test")
        self.assertEqual(keys.stat().st_mode & 0o777, 0o600)
        Server.calls.append({"id": "w1", "kind": "run", "agent": AGENT, "command": "echo hi", "cwd": "/home/user", "seconds": 5})
        base = Path(tempfile.mkdtemp())
        stop = threading.Thread(target=lambda: self.assertRaises(connector.Refused, connector.serve, server, computer, "ckc_wrong", base))
        stop.start()
        stop.join(5)
        worker = threading.Thread(target=connector.serve, args=(server, computer, key, base), daemon=True)
        worker.start()
        for _ in range(50):
            if Server.answers:
                break
            threading.Event().wait(0.1)
        http.shutdown()
        http.server_close()
        self.assertEqual(Server.answers[0]["call"], "w1")
        self.assertEqual(Server.answers[0]["result"]["stdout"], "hi\n")


if __name__ == "__main__":
    unittest.main()
