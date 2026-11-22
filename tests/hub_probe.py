#!/usr/bin/env python3
"""Probe a running hub (`devagotchi hub serve`) over Streamable HTTP MCP.

    python3 tests/hub_probe.py http://127.0.0.1:8787 --token <bearer> --broker <name> [--git /usr/bin/git] [--cwd DIR]
    python3 tests/hub_probe.py http://127.0.0.1:8787 --token <bearer> --broker <name> --expect-error broker_offline
    python3 tests/hub_probe.py http://127.0.0.1:8787 --token <bearer> --broker <name> --expect-inspect-error ipc_io

Checks, in order: an unauthenticated call is refused with a 401 that names the
protected-resource metadata; /healthz answers; initialize and tools/list; whoami names
the broker and `machines` lists it online; `machine_inspect` gets the broker's own
capabilities for its policy; operation_start of `git --version` on that broker succeeds and
its output reads back; a broker the subject holds no grant for is refused with `not_granted`.
With --expect-error CODE it instead checks that `machine_inspect` and one routed call to the
broker fail with exactly CODE (a linked machine that is offline or revoked). With
--expect-inspect-error CODE the machine's link is up but its broker is not: `machines` still
says online, and `machine_inspect` fails with CODE, within its bound. Exit 0 only when every
step holds.
"""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
import uuid

parser = argparse.ArgumentParser()
parser.add_argument("base")
parser.add_argument("--token", required=True)
parser.add_argument("--broker", required=True)
parser.add_argument("--ungranted", default="nobody-has-this")
parser.add_argument("--git", default="/usr/bin/git")
parser.add_argument("--cwd", default=None)
parser.add_argument("--expect-error", default=None)
# The machine's link is up and its broker stopped: what machine_inspect says instead.
parser.add_argument("--expect-inspect-error", default=None)
# A broker under the `account` policy to exercise `run` on (one call: start, wait, output).
parser.add_argument("--run-broker", default=None)
# An absolute directory on that broker's machine (this host in the harness) for the file tools.
parser.add_argument("--run-dir", default=None)
args = parser.parse_args()
base = args.base.rstrip("/")
session = {"id": None, "rid": 0}


def http(method, path, body=None, headers=None, token=args.token):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(base + path, data=data, method=method)
    req.add_header("Accept", "application/json, text/event-stream")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", "Bearer " + token)
    req.add_header("MCP-Protocol-Version", "2025-06-18")
    if session["id"]:
        req.add_header("Mcp-Session-Id", session["id"])
    for k, v in (headers or {}).items():
        req.add_header(k, v)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.status, resp.headers, resp.read()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read()


def rpc(method, params=None):
    session["rid"] += 1
    body = {"jsonrpc": "2.0", "id": session["rid"], "method": method}
    if params is not None:
        body["params"] = params
    status, headers, raw = http("POST", "/mcp", body)
    if status != 200:
        raise SystemExit(f"{method}: HTTP {status}: {raw[:300]!r}")
    sid = headers.get("Mcp-Session-Id")
    if sid and not session["id"]:
        session["id"] = sid
    text = raw.decode()
    if headers.get("Content-Type", "").startswith("text/event-stream"):
        payload = [line[5:].strip() for line in text.splitlines() if line.startswith("data:")]
        text = payload[-1] if payload else "{}"
    message = json.loads(text)
    if "error" in message:
        raise SystemExit(f"{method}: {message['error']}")
    return message["result"]


def call(tool, arguments):
    result = rpc("tools/call", {"name": tool, "arguments": arguments})
    structured = result.get("structuredContent")
    if structured is None:
        structured = json.loads(result["content"][0]["text"])
    return result.get("isError", False), structured


def in_another_session(work):
    """Run `work` in a second MCP session of the same token: another conversation."""
    global session
    first, session = session, {"id": None, "rid": 0}
    try:
        rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                           "clientInfo": {"name": "hub-probe-2", "version": "0"}})
        http("POST", "/mcp", {"jsonrpc": "2.0", "method": "notifications/initialized"})
        return work()
    finally:
        session = first


failures = 0


def check(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name} {detail}".rstrip())
    if not ok:
        failures += 1


status, headers, _ = http("POST", "/mcp", {"jsonrpc": "2.0", "id": 0, "method": "ping"}, token=None)
www = headers.get("WWW-Authenticate", "")
check("unauthenticated refused", status == 401 and "resource_metadata=" in www, f"HTTP {status} {www}")
status, _, _ = http("POST", "/mcp", {"jsonrpc": "2.0", "id": 0, "method": "tools/call", "params": {
    "name": "machine_inspect", "arguments": {"machine": args.broker}}}, token=None)
check("unauthenticated machine_inspect refused", status == 401, f"HTTP {status}")

status, _, raw = http("GET", "/healthz", token=None)
health = json.loads(raw or b"{}")
if args.expect_inspect_error:
    # The harness's hub reaches the same stopped broker on its own host too: healthz says so.
    check("healthz names the stopped broker", status == 503 and health.get("ok") is False
          and any(b.get("reachable") is False for b in health.get("brokers", [])), json.dumps(health)[:200])
else:
    check("healthz", status == 200 and health.get("ok") is True, json.dumps(health)[:200])

init = rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "hub-probe", "version": "0"}})
check("initialize", init["serverInfo"]["name"] == "devagotchi-hub", f"{init['serverInfo']} protocol {init['protocolVersion']}")
http("POST", "/mcp", {"jsonrpc": "2.0", "method": "notifications/initialized"})
if args.expect_inspect_error:
    began = time.monotonic()
    is_error, answer = call("machine_inspect", {"machine": args.broker})
    check(f"machine_inspect says {args.expect_inspect_error}", is_error and answer.get("code") == args.expect_inspect_error,
          json.dumps(answer)[:200])
    check("machine_inspect is bounded", time.monotonic() - began < 6, f"{time.monotonic() - began:.1f}s")
    is_error, seen = call("machines", {})
    mine = [m for m in seen.get("machines", []) if m.get("name") == args.broker] if not is_error else []
    check("machines still says online: the link is up", len(mine) == 1 and mine[0].get("status") == "online",
          json.dumps(seen)[:200])
    sys.exit(1 if failures else 0)
if args.expect_error:
    is_error, answer = call("machine_inspect", {"machine": args.broker})
    check(f"machine_inspect fails with {args.expect_error}", is_error and answer.get("code") == args.expect_error,
          json.dumps(answer)[:200])
    is_error, answer = call("operation_get", {"machine": args.broker,"operation_id": "probe-" + uuid.uuid4().hex})
    check(f"routed call fails with {args.expect_error}", is_error and answer.get("code") == args.expect_error, json.dumps(answer)[:200])
    sys.exit(1 if failures else 0)
listed = rpc("tools/list")["tools"]
tools = sorted(t["name"] for t in listed)
# Directory gate (Claude and OpenAI): every tool has a title and says whether it only reads
# or may destroy, and whether it reaches the open world.
unannotated = [t["name"] for t in listed if not t.get("title") or not (
    "readOnlyHint" in t.get("annotations", {}) and "openWorldHint" in t.get("annotations", {})
    and (t["annotations"]["readOnlyHint"] or "destructiveHint" in t["annotations"]))]
check("every tool has a title and annotations", not unannotated, ", ".join(unannotated))
broker_tools = ["approval_request", "approval_wait", "connection_call", "connections", "create_directory", "dictionary_get", "edit_file", "get_config", "get_file_info", "get_recent_tool_calls",
                "get_usage_stats", "kill_process", "lexicon_handoff", "lexicon_propose", "lexicon_resolve", "lexicon_review", "list_directory", "list_processes", "machine_add", "machine_inspect", "machine_update", "machines", "move_file", "operation_cancel", "operation_get", "operation_heartbeat", "operation_input",
                "operation_list", "operation_read", "operation_resize", "operation_start", "operation_wait", "read_file", "read_image", "read_multiple_files", "read_url", "run", "search_files", "set_config_value", "whoami",
                "workflow_cancel", "workflow_list", "workflow_start", "workflow_status", "workflow_wait",
                "write_file"]
board_tools = [t for t in tools if t.startswith("board_")]
access_tools = ["capabilities_list", "machines_list", "my_grants", "request_access", "wait_for_access"]
check("tools", [t for t in tools if not t.startswith("board_")] == sorted(broker_tools + access_tools) and len(board_tools) == 24,
      ", ".join(tools))

is_error, me = call("whoami", {})
check("whoami", not is_error and args.broker in me.get("brokers", []), json.dumps(me))

is_error, seen = call("machines", {})
mine = [m for m in seen.get("machines", []) if m.get("name") == args.broker] if not is_error else []
check("machines lists the broker online", len(mine) == 1 and mine[0].get("status") == "online", json.dumps(seen)[:200])

# Online is the link; machine_inspect asks the broker itself, which says only what its policy serves.
fields = {"api_version", "version", "os", "arch", "exec_policy", "shell_commands", "workspace_scope", "supported_requests"}
account_only = {"config_set", "process_list", "process_kill", "url_read"}
is_error, caps = call("machine_inspect", {"machine": args.broker})
served = set(caps.get("supported_requests", [])) if not is_error else set()
# The broker named by --broker may run either policy (e2e starts a granted one; the one-line
# join installs an account one), so the answer is checked against the policy it reports.
policy = caps.get("exec_policy") if not is_error else None
check("machine_inspect: the broker's own capabilities", not is_error and set(caps) == fields
      and policy in ("granted", "account")
      and caps.get("shell_commands") is (policy == "account")
      and caps.get("workspace_scope") == ("account" if policy == "account" else "workspace"),
      json.dumps(caps)[:300])
check(f"machine_inspect lists what a {policy} broker answers, and nothing it refuses",
      {"broker_capabilities", "operation_start", "file_read"} <= served
      and (account_only <= served if policy == "account" else not served & account_only),
      json.dumps(caps)[:300])
is_error, denied = call("machine_inspect", {"machine": args.ungranted})
check("machine_inspect refuses an unknown or ungranted machine", is_error
      and denied.get("code") in ("unknown_broker", "not_granted"), json.dumps(denied)[:200])
if args.run_broker:
    is_error, caps = call("machine_inspect", {"machine": args.run_broker})
    check("machine_inspect: an account broker", not is_error and caps.get("exec_policy") == "account"
          and caps.get("shell_commands") is True and caps.get("workspace_scope") == "account"
          and account_only <= set(caps.get("supported_requests", [])), json.dumps(caps)[:300])

is_error, started = call("operation_start", {"machine": args.broker,
    "idempotency_key": str(uuid.uuid4()), "executable": args.git, "args": ["--version"],
    "cwd": args.cwd, "deadline_ms": 30000})
check("operation_start routed", not is_error, json.dumps(started)[:200])
operation = started.get("operation", {}).get("operation_id") if not is_error else None
state = None
if operation:
    # One held request through the hub: the broker answers when the operation settles.
    _, view = call("operation_wait", {"machine": args.broker,"operation_id": operation, "timeout_ms": 30000})
    state = view.get("state")
    _, page = call("operation_read", {"machine": args.broker,"operation_id": operation, "cursor": 0, "max_bytes": 4096})
    text = "".join(f.get("text", "") for f in page.get("frames", []))
    check("git --version under the broker", state == "succeeded" and "git version" in text, f"state={state} output={text.strip()[:60]!r}")

if args.run_broker:
    # One call: the command runs on the machine's shell (bash/sh or PowerShell) and its
    # output and exit code come back in the same answer.
    is_error, ran = call("run", {"machine": args.run_broker, "command": "echo devagotchi-run; exit 3"})
    check("run returns output and exit code in one call",
          not is_error and ran.get("state") == "failed" and ran.get("exit_code") == 3
          and "devagotchi-run" in ran.get("output", ""), json.dumps(ran)[:240])
    # Longer than the wait: still running, with how to follow it; then stopped explicitly.
    is_error, slow = call("run", {"machine": args.run_broker, "command": "sleep 20", "wait_seconds": 1})
    check("run hands back a still-running command", not is_error and slow.get("state") in ("accepted", "running")
          and "operation_read" in (slow.get("next") or ""), json.dumps(slow)[:240])
    if not is_error:
        # A waiter is woken by the cancellation itself, not by a timer.
        began = time.monotonic()
        call("operation_cancel", {"machine": args.run_broker,"operation_id": slow["operation_id"]})
        _, stopped = call("operation_wait", {"machine": args.run_broker,"operation_id": slow["operation_id"], "timeout_ms": 30000})
        check("operation_wait answers when a cancelled command settles",
              stopped.get("state") == "cancelled" and time.monotonic() - began < 15, json.dumps(stopped)[:200])
    # A program that asks for input. The hub names the caller (this account, this MCP
    # session) over whatever a tool call says, and the link refuses any caller no hub can be.
    forged = {"account": "local", "client": "forged"}

    def interactive(command, mode, extra=None):
        return call("operation_start", {"machine": args.run_broker,
            "idempotency_key": str(uuid.uuid4()), "executable": "", "args": [], "command": command,
            "deadline_ms": 120000, "interactive": {"mode": mode}, **(extra or {})})

    def typed(operation_id, text, keys, take_over=False):
        return call("operation_input", {"machine": args.run_broker,
            "operation_id": operation_id, "text": text, "keys": keys, "take_over": take_over})

    def output_of(operation_id):
        _, view = call("operation_wait", {"machine": args.run_broker,"operation_id": operation_id, "timeout_ms": 45000})
        _, page = call("operation_read", {"machine": args.run_broker,"operation_id": operation_id, "cursor": 0, "max_bytes": 4096})
        return view, "".join(f.get("text", "") for f in page.get("frames", []))

    # The machine's shell: PowerShell on Windows (where a console may turn a space into a
    # cursor move, so the size is written with an x), sh elsewhere.
    if os.name == "nt":
        read_line = "$line = [Console]::In.ReadLine(); 'got:' + $line"
        show_size = "'ready'; $null = [Console]::ReadLine(); '{0}x{1}' -f [Console]::WindowHeight, [Console]::WindowWidth"
        new_size = "30x100"
    else:
        read_line, show_size, new_size = "read line; echo got:$line", "echo ready; read x; stty size", "30 100"
    # The arguments are closed (G2): a caller written into a tool call is refused, never used.
    is_error, smuggled = interactive(read_line, "pipe", {"caller": forged})
    check("a tool call that names a caller is refused as invalid_arguments",
          is_error and smuggled.get("code") == "invalid_arguments", json.dumps(smuggled)[:240])
    is_error, piped = interactive(read_line, "pipe")
    check("an interactive start on a pipe", not is_error
          and (piped.get("operation", {}).get("interactive") or {}).get("mode") == "pipe", json.dumps(piped)[:240])
    piped_id = piped.get("operation", {}).get("operation_id", "")
    is_error, other = in_another_session(lambda: typed(piped_id, "from b", ["enter"]))
    check("another conversation does not type into it", is_error and other.get("code") == "not_your_session",
          json.dumps(other)[:200])
    is_error, resized = call("operation_resize", {"machine": args.run_broker,
        "operation_id": piped_id, "cols": 100, "rows": 30})
    check("a pipe has no terminal to resize", is_error and resized.get("code") == "no_terminal",
          json.dumps(resized)[:200])
    is_error, sent = typed(piped_id, "hello", ["enter"])
    view, text = output_of(piped_id)
    def listed(scope):
        _, page = call("operation_list", {"machine": args.run_broker,"scope": scope})
        return [op["operation_id"] for op in page.get("operations", [])]
    others_mine = in_another_session(lambda: listed("mine"))
    check("operation_list shows the account's jobs, and `mine` only this conversation's",
          piped_id in listed("account") and piped_id in listed("mine") and piped_id not in others_mine,
          f"mine elsewhere: {others_mine}")
    check("the conversation that started it types into it", not is_error and view.get("state") == "succeeded"
          and "got:hello" in text, f"{json.dumps(sent)[:120]} state={view.get('state')} output={text[:80]!r}")
    is_error, shell = interactive(show_size, "terminal")
    shell_id = shell.get("operation", {}).get("operation_id", "")
    # Resize only once the program is up: a shell starting up (Windows PowerShell on a slow
    # machine) sets its console's size itself, over one that came before it was ready.
    shown = ""
    for _ in range(90):
        _, page = call("operation_read", {"machine": args.run_broker,
            "operation_id": shell_id, "cursor": 0, "max_bytes": 4096})
        shown = "".join(f.get("text", "") for f in page.get("frames", []))
        if "ready" in shown:
            break
        time.sleep(0.5)
    check("the program on its terminal is up", "ready" in shown, repr(shown[-120:]))
    is_error, taken = in_another_session(lambda: call("operation_resize", {"machine": args.run_broker,
        "operation_id": shell_id, "cols": 100, "rows": 30, "take_over": True}))
    size = taken.get("interactive") or {}
    check("its account's other conversation takes over and resizes it", not is_error
          and (size.get("cols"), size.get("rows")) == (100, 30), json.dumps(taken)[:200])
    in_another_session(lambda: typed(shell_id, "go", ["enter"], take_over=True))
    view, text = output_of(shell_id)
    check("the program sees its new terminal size", view.get("state") == "succeeded" and new_size in text,
          f"state={view.get('state')} output={text[-80:]!r}")
    # A broker that runs only granted programs refuses a command line, in words.
    is_error, refused = call("run", {"machine": args.broker, "command": "echo hi"})
    check("run on a granted-only broker refused", is_error and refused.get("code") == "command_not_allowed",
          json.dumps(refused)[:200])

if args.run_broker and args.run_dir:
    import os
    probe_file = os.path.join(args.run_dir, "devagotchi-probe.txt")
    with open(probe_file, "w", newline="\n") as handle:
        handle.write("devagotchi-file\n")
    is_error, content = call("read_file", {"machine": args.run_broker, "path": probe_file})
    check("read_file returns text in one call", not is_error and content.get("text") == "devagotchi-file\n"
          and content.get("eof") is True, json.dumps(content)[:200])
    is_error, tail = call("read_file", {"machine": args.run_broker, "path": probe_file, "offset": -5})
    check("read_file reads a tail", not is_error and tail.get("text") == "file\n", json.dumps(tail)[:200])
    is_error, listing = call("list_directory", {"machine": args.run_broker, "path": args.run_dir})
    names = {e.get("path"): e.get("kind") for e in listing.get("entries", [])} if not is_error else {}
    check("list_directory lists the file", names.get("devagotchi-probe.txt") == "file", json.dumps(listing)[:200])
    # Write, edit, read back; a stale checksum refuses an overwrite.
    written_file = os.path.join(args.run_dir, "devagotchi-written.txt")
    is_error, made = call("write_file", {"machine": args.run_broker, "path": written_file,
                                         "text": "alpha\nbeta\n", "mode": "create"})
    check("write_file creates", not is_error and made.get("created") is True and len(made.get("sha256", "")) == 64,
          json.dumps(made)[:200])
    is_error, edited = call("edit_file", {"machine": args.run_broker, "path": written_file,
                                          "old_text": "beta", "new_text": "gamma"})
    check("edit_file replaces exactly once", not is_error and edited.get("replacements") == 1, json.dumps(edited)[:200])
    is_error, back = call("read_file", {"machine": args.run_broker, "path": written_file})
    check("the edit reads back", not is_error and back.get("text") == "alpha\ngamma\n", json.dumps(back)[:200])
    # By lines, as Desktop Commander reads: the last line, numbered, with the count.
    is_error, last = call("read_file", {"machine": args.run_broker, "path": written_file, "start_line": -1})
    check("read_file reads the last line", not is_error and last.get("text") == "gamma\n"
          and last.get("start_line") == 2 and last.get("total_lines") == 2, json.dumps(last)[:200])
    is_error, stale = call("write_file", {"machine": args.run_broker, "path": written_file, "text": "lost",
                                          "expected_sha256": made.get("sha256", "")})
    check("a stale checksum refuses the overwrite", is_error and stale.get("code") == "changed_since_read",
          json.dumps(stale)[:200])
    is_error, outside_write = call("write_file", {"machine": args.broker, "path": written_file, "text": "x"})
    check("write_file outside a granted broker's workspace refused",
          is_error and outside_write.get("code") == "path_outside_workspace", json.dumps(outside_write)[:200])
    # An image the AI can see, from the machine.
    picture = os.path.join(args.run_dir, "dot.png")
    call("write_file", {"machine": args.run_broker, "path": picture, "bytes_base64": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="})
    seen = rpc("tools/call", {"name": "read_image", "arguments": {"machine": args.run_broker, "path": picture}})
    blocks = {b.get("type"): b for b in seen.get("content", [])}
    refused = rpc("tools/call", {"name": "read_image", "arguments": {"machine": args.run_broker, "path": probe_file}})
    check("read_image refuses what is not an image", refused.get("isError") is True
          and "not_an_image" in refused["content"][0]["text"], json.dumps(refused)[:200])
    check("read_image returns the image", not seen.get("isError") and blocks.get("image", {}).get("data") == "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="
          and blocks.get("image", {}).get("mimeType") == "image/png", json.dumps(seen)[:240])
    # A directory, a move, info and several files at once, on the machine.
    made_dir = os.path.join(args.run_dir, "probe-dir", "deeper")
    is_error, directory = call("create_directory", {"machine": args.run_broker, "path": made_dir})
    check("create_directory makes the parents", not is_error and directory.get("made") is True
          and directory.get("kind") == "dir", json.dumps(directory)[:200])
    moved_file = os.path.join(made_dir, "moved.txt")
    is_error, moved = call("move_file", {"machine": args.run_broker, "source": written_file,
                                         "destination": moved_file})
    check("move_file moves", not is_error and moved.get("replaced") is False and not os.path.exists(written_file),
          json.dumps(moved)[:200])
    is_error, again = call("move_file", {"machine": args.run_broker, "source": probe_file,
                                         "destination": moved_file})
    check("move_file never replaces without overwrite", is_error and again.get("code") == "already_exists",
          json.dumps(again)[:200])
    is_error, info = call("get_file_info", {"machine": args.run_broker, "path": moved_file})
    check("get_file_info counts lines", not is_error and info.get("kind") == "file" and info.get("lines") == 2,
          json.dumps(info)[:200])
    is_error, found = call("search_files", {"machine": args.run_broker, "path": args.run_dir,
                                            "pattern": "GAMMA", "target": "contents", "context_lines": 1})
    hits = found.get("matches", []) if not is_error else []
    check("search_files finds a line with its context", found.get("complete") is True
          and any(h.get("line") == 2 and h.get("text") == "gamma" and h.get("before") == ["alpha"] for h in hits),
          json.dumps(found)[:300])
    # The machine itself: its settings, and its processes.
    is_error, config = call("get_config", {"machine": args.run_broker})
    check("get_config reads the broker's settings", not is_error and config.get("exec_policy") == "account"
          and "max_concurrent" in config.get("changeable", []), json.dumps(config)[:240])
    is_error, kept = call("set_config_value", {"machine": args.run_broker, "key": "exec_policy", "value": 1})
    check("set_config_value leaves what an agent may run to the operator",
          is_error and kept.get("code") == "setting_not_changeable", json.dumps(kept)[:200])
    # Python is already required by this harness; a Unix `sleep` binary is not.
    marker = f"dvg-hub-process-probe-{uuid.uuid4().hex}"
    outsider = subprocess.Popen([sys.executable, "-c", f"import time; time.sleep(77) # {marker}"])
    try:
        is_error, listed = call("list_processes", {"machine": args.run_broker, "filter": marker})
        check("list_processes finds a process", not is_error
              and any(p.get("pid") == outsider.pid for p in listed.get("processes", [])), json.dumps(listed)[:240])
        is_error, stopped = call("kill_process", {"machine": args.run_broker, "pid": outsider.pid})
        outsider.wait(timeout=5)
    finally:
        if outsider.poll() is None:
            outsider.kill()
            outsider.wait(timeout=5)
    # A polite stop (SIGTERM, -15) where the system has one; Windows has none for another
    # process, so the broker ends it at once and says "kill".
    stopped_as = ("kill", lambda code: code is not None) if os.name == "nt" else ("terminate", lambda code: code == -15)
    check("kill_process stops it", not is_error and stopped.get("exited") is True
          and stopped.get("signal") == stopped_as[0] and stopped_as[1](outsider.returncode),
          f"{json.dumps(stopped)[:200]} returncode={outsider.returncode}")
    is_error, granted = call("list_processes", {"machine": args.broker})
    check("a broker that runs only granted programs keeps to its own jobs",
          is_error and granted.get("code") == "not_permitted", json.dumps(granted)[:200])
    # The web as the machine reaches it: a server on its own localhost, read in pages.
    page_body = ("line %d\n" * 4000) % tuple(range(4000))

    class Page(BaseHTTPRequestHandler):
        def do_GET(self):
            body = page_body.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *_):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Page)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    address = "http://127.0.0.1:%d/page" % server.server_port
    read, offset = "", 0
    for _ in range(10):
        is_error, got = call("read_url", {"machine": args.run_broker, "url": address, "offset": offset})
        if is_error:
            break
        read += got.get("text") or ""
        offset = got.get("next_offset", 0)
        if got.get("eof"):
            break
    check("read_url reads the machine's localhost, page by page", not is_error and read == page_body
          and got.get("status") == 200 and got.get("content_type", "").startswith("text/plain"), json.dumps(got)[:240])
    server.shutdown()
    is_error, granted = call("read_url", {"machine": args.broker, "url": address})
    check("a broker that runs only granted programs reads no web address",
          is_error and granted.get("code") == "not_permitted", json.dumps(granted)[:200])
    # What this caller did, from the hub's audit: newest first, and tallied.
    is_error, recent = call("get_recent_tool_calls", {"tool": "read_url", "max_results": 2})
    calls = recent.get("calls", []) if not is_error else []
    check("get_recent_tool_calls shows the caller's own calls, newest first", len(calls) == 2
          and calls[0].get("machine") == args.broker and calls[0].get("outcome") == "not_permitted"
          and calls[1].get("target") == address and calls[1].get("outcome") == "ok"
          and recent.get("matched", 0) >= 3, json.dumps(recent)[:300])
    is_error, usage = call("get_usage_stats", {})
    tools_used = {t["name"]: t for t in usage.get("tools", [])} if not is_error else {}
    check("get_usage_stats tallies them", "read_url" in tools_used and tools_used["read_url"].get("failed", 0) >= 1
          and any(f.get("name") == "not_permitted" for f in usage.get("failures", [])), json.dumps(usage)[:300])
    is_error, both = call("read_multiple_files", {"machine": args.run_broker,
                                                  "paths": [moved_file, os.path.join(args.run_dir, "missing.txt")]})
    files = both.get("files", []) if not is_error else []
    check("read_multiple_files reads one and reports the other", len(files) == 2
          and (files[0].get("content") or {}).get("text") == "alpha\ngamma\n"
          and (files[1].get("error") or {}).get("code") == "path_not_found", json.dumps(both)[:240])
    # A broker that runs only granted programs reaches files only inside its workspace.
    is_error, outside = call("read_file", {"machine": args.broker, "path": probe_file})
    check("read_file outside a granted broker's workspace refused",
          is_error and outside.get("code") == "path_outside_workspace", json.dumps(outside)[:200])

is_error, denied = call("operation_start", {"machine": args.ungranted,
    "idempotency_key": str(uuid.uuid4()), "executable": args.git, "args": ["--version"], "deadline_ms": 5000})
check("ungranted broker refused", is_error and denied.get("code") in ("not_granted", "unknown_broker"), json.dumps(denied)[:160])

status, _, raw = http("GET", "/healthz", token=None)
health = json.loads(raw or b"{}")
chain = health.get("audit") or {}
check("audit chain verified", chain.get("ok") is True and chain.get("entries", 0) >= 4, json.dumps(chain))

sys.exit(1 if failures else 0)
