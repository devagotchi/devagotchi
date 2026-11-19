#!/usr/bin/env python3
"""A hub from the image hands out builds with nothing configured, and one line joins a machine.

Runs against a deploy/compose stack already up from the hub image this commit built, hosted
(`DEVAGOTCHI_HUB_TENANCY=multi`) with no `downloads` anywhere in its config or environment
(image.yml, `hub image / build`). Then, on this runner, as a person would:

1. `/healthz` reports the builds the image carries (its VERSION, six files), and `/dist`
   serves every file `SHA256SUMS` lists with that checksum: the Linux archive holds the
   `devagotchi` binary and the Windows zip `devagotchi.exe`, for x86_64 and ARM64.
2. A person's AI client connects (dynamic registration, Connect, PKCE) and calls
   `machine_add`. The Windows line's script names the checksum of both Windows builds and
   of the PowerShell installer, as served.
3. The Linux line runs exactly as `machine_add` gave it (`curl -fsSL … | sudo sh`): it
   installs the hub's own build, enrolls the machine and starts it.
4. The machine is online, the AI runs a command on it with `run`, and the machine runs the
   hub's build (`/opt/devagotchi/current` -> `versions/<VERSION>`).
5. With `--docker` (increment 209): the Docker line runs exactly as `machine_add` gave it, in
   an empty directory. It leaves a running machine (broker, link, and an enroll that
   exited 0); the AI runs `echo` on it; `docker compose up -d` a second time keeps it online;
   `machine_update` on it is refused as a container's, and no update starts. The compose
   file names the image of the hub's build, which the job tagged locally from the image
   it just built (a pull request's image is never pushed).

Needs root through passwordless sudo and systemd (a GitHub Ubuntu runner), and Docker with
Compose for `--docker`.

    python3 tests/compose_join.py --hub http://localhost:8787 --version main-abc1234 [--docker]
"""

import argparse
import hashlib
import io
import json
import lzma
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from operator_browser import Person, call  # noqa: E402

ARCHIVES = {
    "devagotchi-x86_64-unknown-linux-gnu.tar.xz": "devagotchi",
    "devagotchi-aarch64-unknown-linux-gnu.tar.xz": "devagotchi",
    "devagotchi-x86_64-pc-windows-msvc.zip": "devagotchi.exe",
    "devagotchi-aarch64-pc-windows-msvc.zip": "devagotchi.exe",
}


def ok(text: str) -> None:
    print(f"PASS {text}", flush=True)


def get(url: str) -> bytes:
    with urllib.request.urlopen(url, timeout=60) as answer:
        assert answer.status == 200, (url, answer.status)
        return answer.read()


def reachable(hub: str, seconds: int = 60) -> bytes:
    """`/healthz` from this host once the published port answers. `docker compose up --wait`
    returns when the container's own health check passes, which is inside the container; the
    host's forwarded port can still reset a connection for a moment after that (seen on the
    ARM runner: ConnectionResetError on the first request). Only connection-level errors are
    retried, each one printed, for at most `seconds`; any HTTP answer is final."""
    deadline = time.monotonic() + seconds
    while True:
        try:
            return get(f"{hub}/healthz")
        except (ConnectionError, urllib.error.URLError) as why:
            if isinstance(why, urllib.error.HTTPError) or time.monotonic() > deadline:
                raise
            print(f"waiting for {hub}/healthz from this host: {why}", flush=True)
            time.sleep(1)


def names_in(name: str, data: bytes) -> set:
    """The base names of the files in one archive."""
    if name.endswith(".zip"):
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            return {Path(entry).name for entry in archive.namelist()}
    with tarfile.open(fileobj=io.BytesIO(lzma.decompress(data))) as archive:
        return {Path(member.name).name for member in archive.getmembers() if member.isfile()}


def image_of(version: str) -> str:
    """The hub image of a build, as `machines/join/compose.rs` names it."""
    if re.fullmatch(r"(main|ci)-[0-9a-f]{7}", version):
        return f"ghcr.io/devagotchi/devagotchi:sha-{version.split('-', 1)[1]}"
    return f"ghcr.io/devagotchi/devagotchi:{version}"


def online(person: Person, name: str, seconds: int = 180) -> dict:
    """`run echo` on `name` once its link has connected and its broker answers."""
    deadline = time.monotonic() + seconds
    while True:
        ran = person.tool("run", {"machine": name, "command": f"echo hi-from-{name}"})
        if not ran.get("isError") and f"hi-from-{name}" in ran["structuredContent"]["output"]:
            return ran
        if time.monotonic() > deadline:
            raise AssertionError(f"{name} never ran a command: {ran}")
        time.sleep(2)


def docker_leg(person: Person, hub: str, version: str) -> None:
    """5. The Docker line leaves a running machine, and a second `up -d` changes nothing."""
    added = person.tool("machine_add", {"name": "docked"})
    assert not added.get("isError"), added
    machine = added["structuredContent"]
    code = machine["code"]
    assert machine["docker"] == (
        f"curl -fsSL {hub}/join/{code}.compose.yml -o compose.yml && docker compose -f compose.yml up -d"
    ), machine
    work = Path(tempfile.mkdtemp(prefix="docked-"))
    compose = ["docker", "compose", "-f", "compose.yml"]

    def logs() -> None:
        shown = subprocess.run(compose + ["logs", "--no-color", "--tail", "200"], cwd=work,
                               capture_output=True, text=True)
        print(shown.stdout[-8000:], shown.stderr[-2000:], file=sys.stderr)

    try:
        up = subprocess.run(["bash", "-c", machine["docker"]], cwd=work, stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=1200)
        print(up.stdout[-4000:], up.stderr[-4000:], file=sys.stderr)
        assert up.returncode == 0, up.returncode
        text = (work / "compose.yml").read_text()
        assert f"FROM {image_of(version)} AS hub" in text, text
        assert text.count(code) == 1 and f'DEVAGOTCHI_ENROLL_CODE: "{code}"' in text, text
        ok(f"the Docker line brought up {image_of(version)} with the code in its environment only")

        online(person, "docked")
        listed = person.tool("machines")["structuredContent"]["machines"]
        assert any(m["name"] == "docked" and m["status"] == "online" for m in listed), listed
        ok("the Docker machine is online and the AI ran a command on it")

        # The same file again: enroll finds its key and exits 0; the machine stays.
        again = subprocess.run(compose + ["up", "-d"], cwd=work, stdin=subprocess.DEVNULL,
                               capture_output=True, text=True, timeout=600)
        print(again.stdout[-2000:], again.stderr[-2000:], file=sys.stderr)
        assert again.returncode == 0, again.returncode
        enroll = subprocess.run(compose + ["ps", "-a", "--format", "json", "enroll"], cwd=work,
                                capture_output=True, text=True, check=True).stdout
        states = [json.loads(line) for line in enroll.splitlines() if line.strip()]
        assert states and all(s.get("ExitCode") == 0 for s in states), states
        online(person, "docked")
        ok("docker compose up -d a second time: enroll exits 0 and the machine stays online")

        refused = person.tool("machine_update", {"machine": "docked"})
        assert refused.get("isError"), refused
        assert "container:" in json.dumps(refused), refused
        listed = person.tool("machines")["structuredContent"]["machines"]
        docked = next(m for m in listed if m["name"] == "docked")
        assert docked["status"] == "online" and not docked.get("update"), docked
        ok("machine_update on the Docker machine is refused as a container's; no update started")
    except BaseException:
        logs()
        raise
    subprocess.run(compose + ["down", "-v"], cwd=work, capture_output=True, timeout=300)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--hub", required=True, help="the hub's public URL")
    parser.add_argument("--version", required=True, help="the VERSION the image carries")
    parser.add_argument("--docker", action="store_true",
                        help="also join a machine with the Docker line (step 5)")
    args = parser.parse_args()
    hub = args.hub.rstrip("/")

    # 1. The builds the image carries, with nothing configured.
    health = json.loads(reachable(hub))
    assert health["downloads"] == {"version": args.version, "files": 6}, health["downloads"]
    sums = {}
    for line in get(f"{hub}/dist/SHA256SUMS").decode().splitlines():
        digest, name = line.split()
        sums[name] = digest
    assert set(sums) == set(ARCHIVES) | {"install-devagotchi.sh", "install-devagotchi.ps1"}, sums
    for name, digest in sums.items():
        data = get(f"{hub}/dist/{name}")
        assert hashlib.sha256(data).hexdigest() == digest, name
        if name in ARCHIVES:
            assert ARCHIVES[name] in names_in(name, data), (name, names_in(name, data))
    ok(f"the image's hub hands out {args.version} with nothing configured: six files, each as listed")

    # 2. A person's AI connects and adds a machine; the Windows line is ready to run.
    person = Person(hub)
    added = person.tool("machine_add", {"name": "joined"})
    assert not added.get("isError"), added
    machine = added["structuredContent"]
    assert machine["linux"] == f"curl -fsSL {hub}/join/{machine['code']} | sudo sh", machine
    assert machine["windows"] == f"irm {hub}/join/{machine['code']}.ps1 | iex", machine
    status, _, script = call(f"{hub}/join/{machine['code']}.ps1")
    assert status == 200, (status, script)
    for name in ["devagotchi-x86_64-pc-windows-msvc.zip", "devagotchi-aarch64-pc-windows-msvc.zip",
                 "install-devagotchi.ps1"]:
        assert sums[name] in script, (name, script)
    assert "$version = '" + args.version + "'" in script, script
    assert not re.search(r"\$sum = ''", script), script
    ok("an AI's machine_add gives both lines; the Windows script names both Windows builds as served")

    # 3. The Linux line, exactly as given (fetching the scripts above spent nothing).
    joined = subprocess.run(["bash", "-c", machine["linux"]], stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=600)
    print(joined.stderr[-4000:], file=sys.stderr)
    assert joined.returncode == 0, joined.stdout[-2000:]
    assert f"installing devagotchi {args.version} and joining {hub}" in joined.stderr, joined.stderr
    ok("the Linux line installed the hub's build and joined")

    # 4. Online, and the AI runs a command on it.
    ran = None
    for _ in range(60):
        ran = person.tool("run", {"machine": "joined", "command": "echo joined-from-the-image"})
        if not ran.get("isError"):
            break
        time.sleep(1)
    assert not ran.get("isError"), ran
    assert "joined-from-the-image" in ran["structuredContent"]["output"], ran
    current = subprocess.run(["sudo", "readlink", "/opt/devagotchi/current"],
                             capture_output=True, text=True, check=True).stdout.strip()
    assert current == f"versions/{args.version}", current
    listed = person.tool("machines")["structuredContent"]["machines"]
    assert any(m.get("status") == "online" for m in listed), listed
    ok(f"the machine is online, runs {args.version}, and the AI ran a command on it")

    if args.docker:
        docker_leg(person, hub, args.version)
    print("COMPOSE JOIN PASSED")
    return 0


if __name__ == "__main__":
    os.umask(0o022)
    sys.exit(main())
