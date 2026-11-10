#!/usr/bin/env python3
"""Probe a running hub's export of board events to ClickHouse (R1 walkthrough step 9).

    DEVAGOTCHI_CLICKHOUSE_PASSWORD=... python3 tests/analytics_probe.py http://127.0.0.1:8787 \
        --token <bearer> --project e2e-board \
        --clickhouse http://127.0.0.1:18123 --user devagotchi --database devagotchi \
        --container dvg-ch

The hub was started with the same DEVAGOTCHI_CLICKHOUSE_* variables. `--token` holds a grant
on `--project`, a configured board. The probe makes tickets and waits for /healthz to say the
export is caught up; stops the ClickHouse container (`docker stop`) and works the board
anyway; sees /healthz report analytics degraded with the events waiting and since when, and
the hub itself still healthy; starts ClickHouse again and waits for the backlog to drain;
then counts, in ClickHouse, the board's events once each (every event id the board's own
history has, no fewer and no extra) and reads the board's statistics through the hub.
Exit 0 only when every step holds.
"""
import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("base")
parser.add_argument("--token", required=True)
parser.add_argument("--project", required=True)
parser.add_argument("--clickhouse", required=True, help="ClickHouse's HTTP interface")
parser.add_argument("--user", default="devagotchi")
parser.add_argument("--database", default="devagotchi")
parser.add_argument("--container", required=True, help="the ClickHouse container to stop and start")
args = parser.parse_args()
base = args.base.rstrip("/")
password = os.environ.get("DEVAGOTCHI_CLICKHOUSE_PASSWORD", "")
failures = 0


def check(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name} {detail}".rstrip(), flush=True)
    if not ok:
        failures += 1


def hub(method, path, body=None, headers=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(base + path, data=data, method=method)
    req.add_header("Authorization", "Bearer " + args.token)
    req.add_header("Accept", "application/json, text/event-stream")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    for name, value in (headers or {}).items():
        req.add_header(name, value)
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return resp.status, resp.headers, resp.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


def health():
    with urllib.request.urlopen(base + "/healthz", timeout=10) as resp:
        return resp.status, json.loads(resp.read())


def health_any():
    try:
        return health()
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read())


class Mcp:
    def __init__(self):
        self.session = None
        self.rid = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                "clientInfo": {"name": "analytics-probe", "version": "0"}})
        self.post({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def post(self, body):
        headers = {"MCP-Protocol-Version": "2025-06-18"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        _, response_headers, raw = hub("POST", "/mcp", body, headers)
        self.session = self.session or response_headers.get("Mcp-Session-Id")
        text = raw.decode()
        if response_headers.get("Content-Type", "").startswith("text/event-stream"):
            data = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
            text = next((d for d in data if d.startswith("{")), "")
        return json.loads(text) if text else {}

    def rpc(self, method, params):
        self.rid += 1
        return self.post({"jsonrpc": "2.0", "id": self.rid, "method": method, "params": params})

    def call(self, tool, arguments):
        answer = self.rpc("tools/call", {"name": tool, "arguments": arguments})
        result = answer.get("result", {})
        structured = result.get("structuredContent")
        if structured is None and result.get("content"):
            structured = json.loads(result["content"][0]["text"])
        return not (answer.get("error") or result.get("isError")), structured


def clickhouse(sql):
    query = urllib.parse.urlencode({"query": sql + " FORMAT JSON",
                                    "output_format_json_quote_64bit_integers": "0"})
    req = urllib.request.Request(f"{args.clickhouse.rstrip('/')}/?{query}", method="POST", data=b"")
    req.add_header("X-ClickHouse-User", args.user)
    req.add_header("X-ClickHouse-Key", password)
    with urllib.request.urlopen(req, timeout=30) as resp:
        return json.loads(resp.read())["data"]


def wait(what, condition, seconds):
    deadline = time.time() + seconds
    last = None
    while time.time() < deadline:
        try:
            last = condition()
            if last:
                return last
        except (OSError, ValueError, KeyError) as error:
            last = error
        time.sleep(1)
    return last if last else None


def caught_up():
    _, body = health_any()
    analytics = body.get("analytics", {})
    return analytics if analytics.get("ready") and analytics.get("pending") == 0 and not analytics.get("degraded") else None


def all_events():
    """Every event the board's own history holds, by sequence."""
    seen, after = [], 0
    while True:
        ok, events = mcp.call("board_events", {"project": args.project, "after_seq": after, "limit": 1000})
        if not ok or not events:
            return seen
        seen.extend(events)
        after = events[-1]["seq"]
        if len(events) < 1000:
            return seen


def docker(*command):
    return subprocess.run(["docker", *command], capture_output=True, text=True, timeout=120).returncode == 0


mcp = Mcp()
_, body = health_any()
check("the hub exports to ClickHouse", body.get("analytics", {}).get("configured") is True, str(body.get("analytics")))
for n in range(3):
    ok, _ = mcp.call("board_create", {"project": args.project, "title": f"analytics probe {n}"})
    check(f"a ticket ({n})", ok)
state = wait("caught up", caught_up, 60)
check("the export catches up", bool(state), str(state))

# ClickHouse goes away: the board keeps working, the events wait, the health page says so.
check("ClickHouse stops", docker("stop", args.container))
for n in range(3, 6):
    ok, _ = mcp.call("board_create", {"project": args.project, "title": f"analytics probe {n}"})
    check(f"the board works with ClickHouse down ({n})", ok)


def degraded():
    status, body = health_any()
    analytics = body.get("analytics", {})
    if analytics.get("degraded") and (analytics.get("pending") or 0) >= 3:
        return status, analytics
    return None


seen = wait("degraded", degraded, 60)
status, analytics = seen if isinstance(seen, tuple) else (None, {})
check("/healthz reports analytics degraded with the events waiting", bool(analytics), str(analytics))
check("and how long the oldest has waited", isinstance(analytics.get("oldest_pending_secs"), int), str(analytics))
check("the hub itself stays healthy", status == 200, str(status))
check("the password is nowhere in it", password == "" or password not in json.dumps(analytics))

# ClickHouse is back: the backlog drains.
check("ClickHouse starts", docker("start", args.container))
check("ClickHouse answers", bool(wait("ping", lambda: clickhouse("SELECT 1 AS one"), 90)))
state = wait("drained", caught_up, 150)
check("the backlog drains", bool(state), str(state))

# Each event counted once, every one of them.
events = all_events()
ids = {f"event:{e['seq']}:{e['hash']}" for e in events}
rows = clickhouse(
    f"SELECT uniqExact(event_id) AS events, count() AS rows FROM {args.database}.board_events FINAL "
    f"WHERE project = '{args.project}' AND owner_id = '@shared'")
held = rows[0] if rows else {}
check("ClickHouse holds every event of the board", held.get("events") == len(ids), f"{held} vs {len(ids)}")
check("and each once", held.get("rows") == held.get("events"), str(held))
status, _, raw = hub("GET", f"/api/v1/projects/{args.project}/stats?days=7")
stats = json.loads(raw) if raw else {}
check("the board's statistics through the hub", status == 200 and stats.get("events") == len(ids), f"{status} {stats.get('events')}")
made = sum(row["events"] for row in stats.get("throughput", []) if row.get("action") == "ticket.created")
check("count the tickets made", made >= 6, str(made))

if failures:
    sys.exit(f"analytics probe: {failures} failed")
print("analytics probe ok", flush=True)
