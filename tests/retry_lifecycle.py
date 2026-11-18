#!/usr/bin/env python3
"""The retry lifecycle (DG-RETRY-1, .ai/docs/retry-lifecycle.md), black box: a real
broker (`devagotchi broker run`) and real stdio MCP adapters (`devagotchi mcp`).

All effects, configuration and journals are confined to a fresh subdirectory of
--work-root. Nothing is installed, enrolled, published or sent to a real service.
Each operation writes its own durable effect file, avoiding shared-append races.
The default 4097 real retirements pass the 4096 keys the journal once kept;
--quick runs 16 and is not that gate.

  python tests/retry_lifecycle.py --devagotchi target/debug/devagotchi --work-root target --quick
"""
import argparse
import collections
import contextlib
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def lines(pipe, output):
    try:
        for line in pipe:
            output.put(line)
    finally:
        output.put(None)


class Mcp:
    def __init__(self, binary, endpoint, env, log):
        self.log = log.open("a", encoding="utf-8")
        self.process = subprocess.Popen(
            [str(binary), "mcp", "--socket", endpoint],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log,
            text=True, encoding="utf-8", bufsize=1, env=env)
        self.queue = queue.Queue()
        threading.Thread(target=lines, args=(self.process.stdout, self.queue), daemon=True).start()
        self.sequence = 0
        self.rpc("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                               "clientInfo": {"name": "retry-lifecycle", "version": "1"}})
        self.rpc("notifications/initialized", notify=True)

    def send(self, method, params=None, notify=False):
        self.sequence += 1
        request = {"jsonrpc": "2.0", "method": method}
        if not notify:
            request["id"] = self.sequence
        if params is not None:
            request["params"] = params
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()
        return self.sequence

    def rpc(self, method, params=None, notify=False):
        seq = self.send(method, params, notify)
        if notify:
            return None
        deadline = time.monotonic() + 60
        while True:
            line = self.queue.get(timeout=max(0.01, deadline - time.monotonic()))
            require(line is not None, "MCP adapter closed unexpectedly")
            response = json.loads(line)
            if response.get("id") == seq:
                require("error" not in response, f"JSON-RPC error: {response.get('error')}")
                return response["result"]

    def call(self, name, arguments, error=None):
        result = self.rpc("tools/call", {"name": name, "arguments": arguments})
        body = result.get("structuredContent")
        if body is None:
            body = json.loads(result["content"][0]["text"])
        if error:
            actual = body.get("code", body.get("error", {}).get("code"))
            require(result.get("isError") and actual == error,
                    f"{name}: wanted {error}, got {body}")
        else:
            require(not result.get("isError"), f"{name}: {body}")
        return body

    def call_many(self, name, arguments):
        ids = [self.send("tools/call", {"name": name, "arguments": item}) for item in arguments]
        pending, results = set(ids), {}
        deadline = time.monotonic() + 60
        while pending:
            line = self.queue.get(timeout=max(0.01, deadline - time.monotonic()))
            require(line is not None, "MCP adapter closed during pipelined calls")
            response = json.loads(line)
            seq = response.get("id")
            if seq not in pending:
                continue
            require("error" not in response, f"JSON-RPC pipeline error: {response.get('error')}")
            result = response["result"]
            body = result.get("structuredContent")
            if body is None:
                body = json.loads(result["content"][0]["text"])
            require(not result.get("isError"), f"{name} pipeline call failed: {body}")
            results[seq] = body
            pending.remove(seq)
        return [results[seq] for seq in ids]

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(10)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(10)
        self.log.close()


class Harness:
    def __init__(self, args):
        self.binary = args.devagotchi.resolve()
        self.work = Path(tempfile.mkdtemp(prefix="retry-lifecycle-", dir=args.work_root.resolve()))
        self.workspace = self.work / "workspace"
        self.workspace.mkdir()
        self.effects = self.workspace / "effects"
        self.effects.mkdir()
        self.state = self.work / "broker-state"
        self.config_path = self.work / "broker.json"
        self.python = str(Path(sys.executable).resolve())
        self.config = {"workspace": str(self.workspace), "allowed_executables": [self.python],
                       "exec_policy": "granted", "output_limit_bytes": 1024,
                       "max_operations": 16, "max_concurrent": 8}
        self.env = {k: v for k, v in os.environ.items()
                    if not k.upper().startswith("DEVAGOTCHI_")}
        self.env.update(TEMP=str(self.work), TMP=str(self.work), TMPDIR=str(self.work))
        self.canary = "synthetic-retry-canary-" + os.urandom(12).hex()
        self.env["DVG_RETRY_TEST_CANARY"] = self.canary
        self.broker = None
        self.mcp = None
        self.log = None
        self.start()

    def start(self):
        self.config_path.write_text(json.dumps(self.config), encoding="utf-8")
        self.log = (self.work / "broker.log").open("a", encoding="utf-8")
        self.broker = subprocess.Popen(
            [str(self.binary), "broker", "run", "--state-dir", str(self.state),
             "--config", str(self.config_path)], stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
            text=True, encoding="utf-8", env=self.env)
        ready = queue.Queue()

        def pump():
            for line in self.broker.stderr:
                self.log.write(line)
                self.log.flush()
                if "ready:" in line:
                    ready.put(line.split("ready:", 1)[1].strip())
            ready.put(None)

        threading.Thread(target=pump, daemon=True).start()
        self.endpoint = ready.get(timeout=30)
        require(self.endpoint is not None, "Broker did not become ready; inspect broker.log")
        self.connect()

    def connect(self):
        self.mcp = Mcp(self.binary, self.endpoint, self.env, self.work / "mcp.log")

    def stop(self):
        if self.mcp:
            self.mcp.stop()
            self.mcp = None
        if self.broker and self.broker.poll() is None:
            self.broker.terminate()
            try:
                self.broker.wait(10)
            except subprocess.TimeoutExpired:
                self.broker.kill()
                self.broker.wait(10)
        self.broker = None
        # Pump finishes after process closes stderr.
        time.sleep(0.05)
        if self.log:
            self.log.close()
            self.log = None

    def restart(self):
        self.stop()
        self.start()

    def request(self, key, delay=0):
        child = ("import sys,time,os; "
                 "f=open(sys.argv[1],'a',encoding='utf-8'); "
                 "f.write(sys.argv[2]+'\\n'); f.flush(); os.fsync(f.fileno()); f.close(); "
                 "time.sleep(float(sys.argv[3]))")
        return {"idempotency_key": key, "executable": self.python,
                "args": ["-c", child, str(self.effects / (key + ".txt")), key, str(delay)],
                "cwd": str(self.workspace), "deadline_ms": 30000}

    def settle(self, operation):
        operation_id = operation["operation_id"]
        for _ in range(10):
            view = self.mcp.call("operation_wait",
                                 {"operation_id": operation_id, "timeout_ms": 5000})
            view = view.get("operation", view)
            if view["state"] not in ("accepted", "running"):
                require(view["state"] == "succeeded" and view["exit_code"] == 0,
                        f"Child did not succeed: {view}")
                return view
        raise AssertionError("Child never settled")

    def settle_many(self, batch):
        ids = {item["operation"]["operation_id"] for item in batch}
        require(len(ids) == len(batch), "Distinct churn keys returned duplicate operation IDs")
        deadline = time.monotonic() + 60
        while True:
            listed = self.mcp.call("operation_list", {"limit": 64})["operations"]
            current = {item["operation_id"]: item for item in listed if item["operation_id"] in ids}
            require(set(current) == ids, "Current batch disappeared from bounded history")
            if all(item["state"] not in ("accepted", "running") for item in current.values()):
                require(all(item["state"] == "succeeded" for item in current.values()),
                        "A churn child did not succeed")
                return
            require(time.monotonic() < deadline, "Churn batch never settled")
            time.sleep(0.01)

    def run(self, request):
        result = self.mcp.call("operation_start", request)
        self.settle(result["operation"])
        return result

    def effect_count(self, key):
        path = self.effects / (key + ".txt")
        return path.read_text(encoding="utf-8").splitlines().count(key) if path.exists() else 0

    def counts(self):
        return collections.Counter(line for path in self.effects.glob("*.txt")
                                   for line in path.read_text(encoding="utf-8").splitlines())

    def replay(self, request, operation_id):
        result = self.run(request)
        require(result["replayed"] and result["operation"]["operation_id"] == operation_id,
                "Retry was not the same recorded operation")
        require(self.effect_count(request["idempotency_key"]) == 1, "Retry duplicated its side effect")


def immutability_probe(h):
    """Another connection, with SQLite's default PRAGMAs, tries to forget, change and
    replace a tombstone inside a retirement-shaped transaction: each fails, and the
    operation that transaction deleted first is still there after it rolls back."""
    database = h.state / "operations.sqlite3"
    # closing(): a sqlite3 connection's own `with` commits but leaves the file open, which
    # Windows will not let the cleanup delete.
    with contextlib.closing(sqlite3.connect(database)) as db:
        key, digest, when = db.execute(
            "SELECT idempotency_key,request_digest,retired_at_ms FROM retired_keys LIMIT 1").fetchone()
        operation_id = db.execute("SELECT operation_id FROM operations LIMIT 1").fetchone()[0]
        for mutation, values in [
            ("DELETE FROM retired_keys WHERE rowid NOT IN "
             "(SELECT rowid FROM retired_keys ORDER BY retired_at_ms DESC,rowid DESC LIMIT 1)", ()),
            ("UPDATE retired_keys SET request_digest='replacement' WHERE idempotency_key=?", (key,)),
            ("INSERT OR REPLACE INTO retired_keys(idempotency_key,request_digest,retired_at_ms) "
             "VALUES(?,?,?)", (key, digest, when)),
        ]:
            db.execute("BEGIN")
            db.execute("DELETE FROM output_frames WHERE operation_id=?", (operation_id,))
            db.execute("DELETE FROM operations WHERE operation_id=?", (operation_id,))
            rejected = False
            try:
                db.execute(mutation, values)
            except sqlite3.IntegrityError:
                rejected = True
            finally:
                db.rollback()
            require(rejected, "Another connection changed a tombstone: " + mutation)
            require(db.execute("SELECT COUNT(*) FROM operations WHERE operation_id=?",
                               (operation_id,)).fetchone()[0] == 1,
                    "A failed retirement lost its operation")
    print("PASS tombstones refuse delete, update and replace; the transaction rolls back", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--devagotchi", required=True, type=Path)
    parser.add_argument("--work-root", required=True, type=Path)
    parser.add_argument("--retirements", type=int, default=4097)
    parser.add_argument("--keep", action="store_true")
    parser.add_argument("--quick", action="store_true", help="16 retirements only; not the overflow gate")
    args = parser.parse_args()
    require(args.devagotchi.is_file(), "Binary does not exist")
    require(args.work_root.is_dir(), "--work-root must be an existing disposable workspace directory")
    if args.quick:
        args.retirements = 16
        print("QUICK CHECK: skips the >4096 retirement overflow gate", flush=True)
    else:
        require(args.retirements >= 4097, "Use at least 4097 retirements to pass 4096")
    binary_sha = hashlib.sha256(args.devagotchi.read_bytes()).hexdigest()
    h = Harness(args)
    passed = False
    print(f"Binary SHA256 {binary_sha}\nWorkspace {h.work}", flush=True)
    try:
        first = h.request("oldest-side-effect")
        initial = h.run(first)
        h.replay(first, initial["operation"]["operation_id"])
        churn_count = args.retirements + h.config["max_operations"] - 1
        for start in range(0, churn_count, 8):
            batch = h.mcp.call_many("operation_start",
                                    [h.request(f"churn-{number}")
                                     for number in range(start, min(start + 8, churn_count))])
            h.settle_many(batch)
            if start % 512 == 0:
                print(f"Completed {min(start + 8, churn_count)} churn operations", flush=True)
        expected = collections.Counter(["oldest-side-effect"] +
                                       [f"churn-{number}" for number in range(churn_count)])
        require(h.counts() == expected, "Churn must produce every expected per-key effect exactly once")
        with contextlib.closing(sqlite3.connect(
                "file:" + (h.state / "operations.sqlite3").as_posix() + "?mode=ro", uri=True)) as db:
            retired = db.execute("SELECT COUNT(*) FROM retired_keys").fetchone()[0]
        require(retired == args.retirements, f"Expected {args.retirements} tombstones, got {retired}")
        print(f"Verified {len(expected)} exact per-key effects and {retired} durable tombstones", flush=True)
        h.mcp.call("operation_get", {"operation_id": initial["operation"]["operation_id"]},
                   error="not_found")
        h.mcp.call("operation_start", first, error="idempotency_key_retired")
        require(h.effect_count("oldest-side-effect") == 1, "Retired retry duplicated side effect")
        h.restart()
        h.mcp.call("operation_start", first, error="idempotency_key_retired")
        require(h.effect_count("oldest-side-effect") == 1, "Restart forgot retired retry protection")
        print(f"PASS {args.retirements} retirements and restart preserve once-only effects", flush=True)

        stable = h.request("stable-across-tuning")
        recorded = h.run(stable)["operation"]["operation_id"]
        for field, value in [("args", stable["args"] + ["changed"]), ("deadline_ms", 29999)]:
            h.mcp.call("operation_start", {**stable, field: value}, error="idempotency_conflict")
        h.config.update(max_operations=4, max_concurrent=2, output_limit_bytes=4096)
        h.config["allowed_executables"].append(str(h.binary))
        h.restart()
        h.replay(stable, recorded)
        h.config["allowed_executables"] = [str(h.binary)]
        h.restart()
        h.mcp.call("operation_start", stable, error="executable_not_allowed")
        require(h.effect_count("stable-across-tuning") == 1, "Revoked grant produced an effect")
        h.config["allowed_executables"] = [h.python]
        h.restart()
        h.replay(stable, recorded)
        print("PASS tuning/unrelated grants preserve identity; changed input/revocation refused", flush=True)

        uncertain = h.request("disconnect-after-effect", delay=2)
        h.mcp.send("tools/call", {"name": "operation_start", "arguments": uncertain})
        end = time.monotonic() + 15
        while h.effect_count("disconnect-after-effect") != 1 and time.monotonic() < end:
            time.sleep(0.01)
        require(h.effect_count("disconnect-after-effect") == 1, "Disconnect fixture never produced its effect")
        h.mcp.stop()  # Deliberately abandon the unread admission response.
        h.connect()
        replay = h.run(uncertain)
        require(replay["replayed"] and h.effect_count("disconnect-after-effect") == 1,
                "Adapter reconnect duplicated uncertain admission")
        h.restart()
        h.replay(uncertain, replay["operation"]["operation_id"])
        print("PASS unread response, adapter reconnect and broker restart keep one effect", flush=True)

        h.config.update(max_operations=2, max_concurrent=1)
        h.restart()
        kept_request = h.request("history-kept-on-refusal")
        kept_id = h.run(kept_request)["operation"]["operation_id"]
        active = h.mcp.call("operation_start", h.request("concurrency-active", delay=2))
        h.mcp.call("operation_start", h.request("refused-no-effect"),
                   error="concurrency_limit_reached")
        h.mcp.call("operation_get", {"operation_id": kept_id})
        require(h.effect_count("refused-no-effect") == 0, "Refused admission had an effect")
        h.settle(active["operation"])
        h.config["max_operations"] = 1
        h.restart()
        h.run(h.request("after-history-shrink"))
        h.mcp.call("operation_get", {"operation_id": kept_id}, error="not_found")
        h.mcp.call("operation_start", kept_request, error="idempotency_key_retired")
        require(h.effect_count("history-kept-on-refusal") == 1, "History shrink lost retry protection")
        print("PASS refused admission retains history; shrinking retires without replay", flush=True)

        h.config.update(max_operations=4, max_concurrent=2)
        h.restart()
        crash_request = h.request("crash-after-effect", delay=5)
        crash_started = h.mcp.call("operation_start", crash_request)
        end = time.monotonic() + 15
        while h.effect_count("crash-after-effect") != 1 and time.monotonic() < end:
            time.sleep(0.01)
        require(h.effect_count("crash-after-effect") == 1, "Crash fixture never produced its effect")
        h.broker.kill()  # Only this fixture's disposable broker, not an installed process.
        h.broker.wait(10)
        h.restart()
        recovered = h.mcp.call("operation_start", crash_request)
        require(recovered["replayed"] and recovered["operation"]["state"] == "unknown"
                and recovered["operation"]["operation_id"]
                == crash_started["operation"]["operation_id"],
                "Unsettled crash was not recovered as the same unknown operation")
        require(h.effect_count("crash-after-effect") == 1, "Crash recovery replayed an effect")
        print("PASS broker crash after effect preserves unknown outcome without execution", flush=True)

        canary_request = h.request("canary-profile-check")
        canary_request["args"] = ["-c", "import os; print('present' if "
                                  "'DVG_RETRY_TEST_CANARY' in os.environ else 'absent')"]
        canary_op = h.run(canary_request)["operation"]["operation_id"]
        output = h.mcp.call("operation_read", {"operation_id": canary_op, "cursor": 0,
                                              "max_bytes": 4096})
        require("absent" in "".join(frame["text"] for frame in output["frames"]),
                "Granted execution leaked inherited synthetic environment canary")
        h.stop()
        for path in h.work.rglob("*"):
            if path.is_file():
                require(h.canary.encode() not in path.read_bytes(),
                        "Synthetic secret canary leaked to fixture storage: " + str(path))
        print("PASS synthetic environment canary absent from child output, logs and journal", flush=True)
        immutability_probe(h)
        expected.update(["stable-across-tuning", "disconnect-after-effect", "history-kept-on-refusal",
                         "concurrency-active", "after-history-shrink", "crash-after-effect"])
        require(h.counts() == expected, "Final per-key effects must exactly match the expected set")
        print(f"RETRY LIFECYCLE {'QUICK CHECK' if args.quick else 'OVERFLOW GATE'} PASSED: "
              f"{sum(h.counts().values())} unique real child effects", flush=True)
        require(hashlib.sha256(args.devagotchi.read_bytes()).hexdigest() == binary_sha,
                "Candidate binary changed during verification")
        passed = True
        return 0
    finally:
        h.stop()
        if args.keep or not passed:
            print(f"Retained evidence: {h.work}", flush=True)
        else:
            # tempfile created this exact child beneath caller's explicitly supplied root.
            require(h.work.parent == args.work_root.resolve(), "Refusing cleanup outside work root")
            shutil.rmtree(h.work)


if __name__ == "__main__":
    sys.exit(main())
