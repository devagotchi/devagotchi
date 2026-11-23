#!/usr/bin/env python3
"""Actual self-hosted OAuth -> grant -> hub -> link -> broker file acceptance.

Requires a disposable PostgreSQL database in DEVAGOTCHI_HUB_STORAGE_URL and a built
binary. Uses public owner/client/access APIs, never seeded grants or real user data.
"""
import argparse
import base64
import hashlib
import json
import os
import re
import subprocess
import tempfile
import time
from pathlib import Path

from analytics_browser import start
from operator_browser import Person, free_port, wait_ready
from compose_join_access import human, owner_cookie, register


def tool(person, name, **args):
    result = person.tool(name, {"machine": "files", **args})
    assert not result.get("isError"), (name, result)
    return result.get("structuredContent", result)


def grant(person, key, capability="fs.write", duration="1h"):
    result = tool(person, "request_access", request_id=key,
                  selection={"kind": "items", "items": [{"capability": capability,
                      "sandbox_level": "unsandboxed", "selector": {}}]},
                  duration=duration, reason="Disposable real file-access acceptance.")
    group = result["result"]
    pending = human(person, "/api/v1/access-requests?status=pending")["rows"]
    item = next(row for row in pending if row["id"] == group["request_ids"][0])
    human(person, f"/api/v1/access-requests/{item['id']}/approve",
          {"version": item["version"], "note": "Accept this disposable test."}, key=key + "-approve")
    rows = human(person, "/api/v1/grants?live=true")["rows"]
    return next(row for row in rows if row["request_id"] == item["id"])


def revoke(person, row, key):
    human(person, f"/api/v1/grants/{row['id']}/revoke", {"version": row["version"]}, key=key)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--devagotchi", required=True)
    args = parser.parse_args()
    assert os.environ.get("DEVAGOTCHI_HUB_STORAGE_URL", "").startswith(("postgres://", "postgresql://")), "disposable PostgreSQL required"
    binary = str(Path(args.devagotchi).resolve())
    with tempfile.TemporaryDirectory(prefix="dvg-file-access-") as temporary:
        work = Path(temporary)
        port = free_port()
        url = f"http://localhost:{port}"
        config, state = work / "hub.json", work / "hub-state"
        config.write_text(json.dumps({"listen": f"127.0.0.1:{port}", "public_url": url,
            "auth": {"mode": "hub"}, "tenancy": {"mode": "single"},
            "machines": {"auto_update": False}, "brokers": [], "grants": []}))
        processes = []
        try:
            hub, _ = start([binary, "hub", "serve", "--state-dir", str(state), "--config", str(config)], "hub", work)
            processes.append(hub)
            wait_ready(url, hub, work / "hub.log")
            owner = subprocess.run([binary, "hub", "owner", "--state-dir", str(state),
                "--config", str(config), "--name", "File acceptance owner"],
                capture_output=True, text=True, check=True, timeout=30).stdout.strip()
            person = Person(url, cookie=owner_cookie(url, owner))
            added_result = person.tool("machine_add", {"name": "files"})
            assert not added_result.get("isError"), added_result
            added = added_result["structuredContent"]
            code = added["code"]
            device = re.search(r"--device ([a-z0-9-]+) --broker", added["manual"]).group(1)
            workspace = work / "workspace"
            workspace.mkdir()
            (workspace / "a.txt").write_bytes(b"alpha\nbeta\n")
            (workspace / "b.txt").write_text("second")
            # A valid PNG spanning several broker pages: image transport returns identical bytes.
            image = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jRZkAAAAASUVORK5CYII=") + bytes(60_000)
            (workspace / "image.png").write_bytes(image)
            broker_config = work / "broker.json"
            broker_config.write_text(json.dumps({"workspace": str(workspace), "exec_policy": "account",
                "output_limit_bytes": 65536, "max_operations": 64, "max_concurrent": 2}))
            broker, ready = start([binary, "broker", "run", "--state-dir", str(work / "broker-state"),
                "--config", str(broker_config)], "broker", work)
            processes.append(broker)
            endpoint = ready.split("ready:", 1)[1].strip()
            subprocess.run([binary, "link", "enroll", "--hub", url, "--device", device,
                "--broker", endpoint, "--state-dir", str(work / "link-state")],
                capture_output=True, text=True, check=True, timeout=60,
                env={**os.environ, "DEVAGOTCHI_ENROLL_CODE": code})
            link, _ = start([binary, "link", "run", "--state-dir", str(work / "link-state")], "link", work)
            processes.append(link)
            for _ in range(60):
                listed = person.tool("machines")["structuredContent"]["machines"]
                if any(row["name"] == "files" and row["status"] == "online" for row in listed):
                    break
                time.sleep(.2)
            else:
                raise AssertionError("machine never online")
            refused = person.tool("read_file", {"machine": "files", "path": "a.txt"})
            assert refused.get("isError") and refused["structuredContent"]["code"] == "not_granted", refused
            register(person, "files")
            once = grant(person, "batch-once", "fs.read", "once")
            batch = tool(person, "read_multiple_files", paths=["a.txt", "b.txt"])
            assert [row["content"]["text"] for row in batch["files"]] == ["alpha\nbeta\n", "second"], batch
            refused = person.tool("read_file", {"machine": "files", "path": "a.txt"})
            assert refused.get("isError") and refused["structuredContent"]["code"] == "grant_required", refused
            print("PASS once-grant batch reads both files; next invocation refuses", flush=True)
            grant(person, "image-once", "fs.read", "once")
            result = person.tool("read_image", {"machine": "files", "path": "image.png"})
            assert not result.get("isError"), result
            actual = next(item for item in result["content"] if item["type"] == "image")
            assert base64.b64decode(actual["data"]) == image
            refused = person.tool("read_image", {"machine": "files", "path": "image.png"})
            assert refused.get("isError") and refused["structuredContent"]["code"] == "grant_required", refused
            print("PASS multi-page image spends one use and returns identical bytes", flush=True)
            live = grant(person, "file-work")
            assert tool(person, "read_file", path="a.txt", max_bytes=2).get("sha256") is None
            info = tool(person, "get_file_info", path="a.txt", sha256=True)
            assert info["sha256"] == hashlib.sha256(b"alpha\nbeta\n").hexdigest()
            assert tool(person, "list_directory", path=".")["entries"]
            assert tool(person, "search_files", path=".", pattern="alpha", target="contents")["matches"]
            made = tool(person, "write_file", path="new/deep.txt", text="before", mode="create", create_dirs=True, idempotency_key="create")
            assert made["sha256"] == hashlib.sha256(b"before").hexdigest()
            # An external editor after a lost reply must not be overwritten by replay.
            (workspace / "new/deep.txt").write_text("external")
            replay = tool(person, "write_file", path="new/deep.txt", text="before", mode="create", create_dirs=True, idempotency_key="create")
            assert replay == made and (workspace / "new/deep.txt").read_text() == "external"
            edit = tool(person, "edit_file", path="new/deep.txt", old_text="external", new_text="after", idempotency_key="edit")
            tool(person, "create_directory", path="destination", idempotency_key="directory")
            tool(person, "move_file", source="new/deep.txt", destination="destination/moved.txt", expected_sha256=edit["sha256"], idempotency_key="move")
            tool(person, "delete_file", path="destination/moved.txt", expected_sha256=edit["sha256"], idempotency_key="delete")
            tool(person, "delete_file", path="destination/moved.txt", expected_sha256=edit["sha256"], idempotency_key="delete")
            assert not (workspace / "destination/moved.txt").exists()
            print("PASS ordinary read/list/info/search and guarded create/edit/mkdir/move/delete with durable replay", flush=True)
            revoke(person, live, "revoke-files")
            for name, arguments in [("read_file", {"path":"a.txt"}), ("read_multiple_files", {"paths":["a.txt","b.txt"]}),
                                    ("read_image", {"path":"image.png"}), ("write_file", {"path":"blocked", "text":"never", "mode":"create", "idempotency_key":"blocked"})]:
                result = person.tool(name, {"machine":"files", **arguments})
                assert result.get("isError") and result["structuredContent"]["code"] == "grant_required", (name,result)
            assert not (workspace / "blocked").exists()
            print("PASS revoked grant blocks ordinary/composite reads and mutation effects", flush=True)
        except BaseException:
            for name in ["hub", "broker", "link"]:
                path = work / f"{name}.log"
                if path.exists():
                    print(f"{name} log tail: {path.read_text()[-3000:]}")
            raise
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
