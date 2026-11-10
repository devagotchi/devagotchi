#!/usr/bin/env python3
"""Probe a running hub's signal sink (docs/signals.md, increment 117).

    python3 tests/signals_probe.py http://127.0.0.1:8787 --port 18999 --token T --secret S

The hub was started with one sink, `metadata`, at http://127.0.0.1:<port>/signals, naming
environment variables that hold `--token` and `--secret`, and nothing listened on that port
while the other probes worked the hub. So first: `/healthz` answered 200 throughout and
reports the sink degraded, behind, and never its token or secret. Then this probe listens
on the port, and the hub delivers: every signal once (no id twice), each batch carrying the
token and a valid signature, every signal following `devagotchi.signal/v1` with no `data`;
the board's events, the tenants' chains (organisations, members) and the audit chain's
tool calls with their refusals all arrive; and `/healthz` says the sink caught up.
Exit 0 only when every step holds.
"""
import argparse
import hashlib
import hmac
import http.server
import json
import sys
import threading
import time
import urllib.error
import urllib.request

parser = argparse.ArgumentParser()
parser.add_argument("base")
parser.add_argument("--port", type=int, required=True)
parser.add_argument("--token", required=True)
parser.add_argument("--secret", required=True)
args = parser.parse_args()
base = args.base.rstrip("/")
failures = 0
received, problems = [], []
lock = threading.Lock()
REQUIRED = {"schema", "id", "source", "kind", "at_ms"}
KNOWN = REQUIRED | {"tenant", "owner", "project", "actor", "ticket", "target", "operation", "outcome", "data"}


def check(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name} {detail}".rstrip(), flush=True)
    if not ok:
        failures += 1


def health():
    try:
        with urllib.request.urlopen(base + "/healthz", timeout=10) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read())


class Sink(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        expected = "sha256=" + hmac.new(args.secret.encode(), body, hashlib.sha256).hexdigest()
        with lock:
            if self.headers.get("Authorization") != f"Bearer {args.token}":
                problems.append("a batch without the token")
            if not hmac.compare_digest(self.headers.get("Devagotchi-Signature", ""), expected):
                problems.append("a batch whose signature does not verify")
            if self.headers.get("Devagotchi-Signal-Schema") != "devagotchi.signal/v1":
                problems.append("a batch without the schema header")
            for line in body.decode().splitlines():
                if line.strip():
                    received.append(json.loads(line))
        self.send_response(202)
        self.end_headers()

    def log_message(self, *_):
        pass


# While nothing listened: the hub kept serving and said the sink is behind.
status, body = health()
sink = (body.get("signals") or [{}])[0]
check("the hub stayed healthy with its sink down", status == 200, str(status))
check("/healthz reports the sink degraded", sink.get("degraded") is True and sink.get("ready") is False, str(sink))
check("and behind", (sink.get("waiting") or 0) > 0 and isinstance(sink.get("oldest_waiting_secs"), int), str(sink))
check("and never its token or secret", args.token not in json.dumps(body) and args.secret not in json.dumps(body))

server = http.server.ThreadingHTTPServer(("127.0.0.1", args.port), Sink)
threading.Thread(target=server.serve_forever, daemon=True).start()


def caught_up():
    _, body = health()
    sink = (body.get("signals") or [{}])[0]
    return sink if sink.get("ready") and not sink.get("degraded") and sink.get("waiting") == 0 else None


deadline, state = time.time() + 150, None
while time.time() < deadline and not state:
    state = caught_up()
    time.sleep(1)
time.sleep(3)  # a round that started before caught_up() was read
check("the sink caught up once it answered", bool(state), str(state))
server.shutdown()

with lock:
    signals = list(received)
ids = [s.get("id") for s in signals]
check("signals arrived", len(signals) > 20, str(len(signals)))
check("each once", len(ids) == len(set(ids)), f"{len(ids)} sent, {len(set(ids))} distinct")
check("every batch carried the token and a valid signature", not problems, "; ".join(sorted(set(problems))))
check("every signal follows devagotchi.signal/v1",
      all(s.get("schema") == "devagotchi.signal/v1" and REQUIRED <= s.keys() and s.keys() <= KNOWN for s in signals))
check("a metadata sink gets no data", all("data" not in s for s in signals))
by_source = {source: [s for s in signals if s.get("source") == source] for source in ("board", "tenancy", "audit")}
check("board events, from the configured board",
      any(s["kind"] == "ticket.created" and s.get("owner") == "@shared" and s.get("project") == "e2e-board" for s in by_source["board"]))
check("board events, from an organisation's project",
      any(s.get("project") == "web" and (s.get("owner") or "").startswith("org-") for s in by_source["board"]))
check("the tenants' chains: an organisation made and a member set",
      {"org.created", "org.member_set"} <= {s["kind"] for s in by_source["tenancy"]})
tools = by_source["audit"]
check("the audit chain's tool calls", any(s["kind"] == "tool.board_create" for s in tools), str(len(tools)))
check("with their refusals", any(s.get("outcome") not in (None, "ok") for s in tools),
      str(sorted({s.get("outcome") for s in tools}))[:200])
in_order = all(
    [s["id"] for s in by_source["audit"]] == sorted((s["id"] for s in by_source["audit"]), key=lambda i: int(i.split(":")[1]))
    for _ in [0]
)
check("the audit chain in order", in_order)

if failures:
    sys.exit(f"signals probe: {failures} failed")
print("signals probe ok", flush=True)
