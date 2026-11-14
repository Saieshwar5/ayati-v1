#!/usr/bin/env python3
"""A tiny MCP server a company could connect to a Cuckoo agent.

StudyHub is the tutoring app in the connected systems guide
(runtime/connected-systems-guide.md). This example uses only the Python standard
library, answers Streamable HTTP MCP with plain JSON replies, and keeps its data
in memory. Run it, put HTTPS in front of it, and connect it from the agent's
Tools tab:

    STUDYHUB_KEY=choose-a-long-random-key python3 tools/examples/studyhub_mcp.py

It listens on 127.0.0.1:8765 at /mcp. A real server checks its own users,
stores data properly and logs nothing private.
"""
import hmac
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

KEY = os.environ.get("STUDYHUB_KEY", "")
MARKS = {"stu-104": {"Maths": 41, "Science": 72}, "stu-221": {"Maths": 88, "Science": 64}}
PLANS = {}

# One tool per action. Plain names and descriptions, flat inputs, and a hint
# saying whether the tool only reads. Cuckoo shows these to the person who
# connects the system and suggests Allow for reads and Ask first for changes.
TOOLS = [
    {
        "name": "get_marks",
        "title": "Read exam marks",
        "description": "A student's latest exam marks by subject.",
        "annotations": {"readOnlyHint": True},
        "inputSchema": {
            "type": "object",
            "properties": {"student": {"type": "string", "description": "The student's id, such as stu-104"}},
            "required": ["student"],
        },
    },
    {
        "name": "save_study_plan",
        "title": "Save a study plan",
        "description": "Saves a study plan for a student. Changes StudyHub.",
        "annotations": {"readOnlyHint": False},
        "inputSchema": {
            "type": "object",
            "properties": {
                "student": {"type": "string"},
                "plan": {"type": "string", "maxLength": 2000},
            },
            "required": ["student", "plan"],
        },
    },
]


def call(name, arguments):
    student = arguments.get("student", "")
    if name == "get_marks":
        if student not in MARKS:
            return {"isError": True, "content": [{"type": "text", "text": "No such student."}]}
        marks = MARKS[student]
        return {"content": [{"type": "text", "text": json.dumps(marks)}], "structuredContent": marks}
    if name == "save_study_plan":
        PLANS[student] = arguments.get("plan", "")
        return {"content": [{"type": "text", "text": f"Plan saved for {student}."}], "structuredContent": {"saved": True}}
    return {"isError": True, "content": [{"type": "text", "text": "Unknown tool."}]}


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/mcp":
            return self.send_error(404)
        given = self.headers.get("Authorization", "")
        if KEY and not hmac.compare_digest(given, f"Bearer {KEY}"):
            return self.send_error(401)
        length = int(self.headers.get("Content-Length", "0"))
        if length > 65536:
            return self.send_error(413)
        message = json.loads(self.rfile.read(length) or b"{}")
        method = message.get("method")
        if "id" not in message:  # a notification, such as notifications/initialized
            self.send_response(202)
            self.end_headers()
            return
        if method == "initialize":
            result = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                      "serverInfo": {"name": "studyhub", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": TOOLS}
        elif method == "tools/call":
            params = message.get("params", {})
            result = call(params.get("name"), params.get("arguments") or {})
        else:
            reply = {"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32601, "message": "Unknown method"}}
            return self.reply(reply)
        self.reply({"jsonrpc": "2.0", "id": message["id"], "result": result})

    def reply(self, body):
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass  # never log requests: they carry students' data


if __name__ == "__main__":
    if not KEY:
        raise SystemExit("Set STUDYHUB_KEY to a long random key first.")
    ThreadingHTTPServer(("127.0.0.1", 8765), Handler).serve_forever()
