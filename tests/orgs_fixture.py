#!/usr/bin/env python3
"""Run the organisation probe with hosted accounts and their real browser/OAuth sessions.

The surrounding E2E run supplies its disposable Postgres database. Sharing that database
lets its signal sink check these organisation events too, without enabling hosted routes
on the single-environment development-token fixture.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from operator_browser import Person, free_port, wait_ready


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--devagotchi", required=True, type=Path)
    parser.add_argument("--shared", required=True)
    args = parser.parse_args()
    binary = args.devagotchi.resolve()
    if not os.environ.get("DEVAGOTCHI_HUB_STORAGE_URL"):
        parser.error("DEVAGOTCHI_HUB_STORAGE_URL must name the disposable E2E database")
    with tempfile.TemporaryDirectory(prefix="devagotchi-orgs-") as directory:
        work = Path(directory)
        port = free_port()
        base = f"http://127.0.0.1:{port}"
        config = {
            "listen": f"127.0.0.1:{port}", "public_url": base,
            "auth": {"mode": "hub", "rp_id": "127.0.0.1"},
            "tenancy": {"mode": "multi"},
            "boards": [{"project": args.shared}], "account_boards": {},
            "brokers": [], "grants": [],
        }
        config_path = work / "hub.json"
        log_path = work / "hub.log"
        process = None

        def stop():
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)

        with log_path.open("w", encoding="utf-8") as log:
            def start():
                config_path.write_text(json.dumps(config), encoding="utf-8")
                child = subprocess.Popen(
                    [str(binary), "hub", "serve", "--state-dir", str(work / "state"),
                     "--config", str(config_path)], stdout=log, stderr=log,
                )
                return child

            try:
                process = start()
                wait_ready(base, process, log_path)
                people = {name: Person(base) for name in ("owner", "member", "operator")}
                operator = people["operator"].tool("whoami")["structuredContent"]["subject"]
                stop()
                config["grants"] = [{"subject": operator, "boards": [args.shared], "human": True}]
                process = start()
                wait_ready(base, process, log_path)
                credentials = work / "people.json"
                credentials.write_text(json.dumps({
                    "tokens": {name: person.token for name, person in people.items()},
                    "cookies": {name: person.cookie for name, person in people.items()},
                }), encoding="utf-8")
                if os.name != "nt":
                    credentials.chmod(0o600)
                return subprocess.run([
                    sys.executable, str(Path(__file__).with_name("orgs_probe.py")), base,
                    "--shared", args.shared, "--people-file", str(credentials),
                ], timeout=180).returncode
            finally:
                stop()


if __name__ == "__main__":
    sys.exit(main())
