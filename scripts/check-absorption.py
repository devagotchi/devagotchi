"""Verify preserved-source accounting without changing Git, running CI or closing rows.

Pass --go-source and --rust-source to also compare the immutable Git objects. Without
them this validates the committed ledger's structure and evidence requirements only.
--require-closed is for final delivery, never a way to skip unauthored requirements.
"""

import argparse
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = ROOT / ".ai/dev/plans/absorption"
PACKETS = {f"A{i:02}" for i in range(13)} | {"B01", "C01", "D01", "E01"}
CLOSED = {"implemented", "equivalent", "superseded_in_source", "archival"}
STATES = CLOSED | {"unverified", "planned", "authored", "partial", "deferred"}
EXPECTED = [
    ("46419c697964016eb547c60416a8f2f5e854a95f", 91, 737),
    ("ca626db10456b8597704032e80944b359b892312", 2, 17),
    ("97172d9a3388afb6c8800b4b28d1ecb3487b4eab", 5, 79),
    ("adca8b75ffb715a137d94166bf2677f13e5da5ab", 1, 32),
    ("c936f7f5f4d46032a222cfc0431defb373d5ddb5", 1, 42),
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args])


def tree(repo, sha):
    result = {}
    for row in git(repo, "ls-tree", "-rz", sha).split(b"\0"):
        if row:
            meta, path = row.split(b"\t", 1)
            mode, kind, blob = meta.decode().split()
            result[path.decode()] = (mode, kind, blob)
    return result


def safe_path(value):
    path = PurePosixPath(value)
    return value and not path.is_absolute() and ".." not in path.parts and "\\" not in value


def verify_source(data, repo):
    sha, previous = data["sha"], data["previous"]
    entries = tree(repo, sha)
    require(len(entries) == data["head_path_count"], f"{sha}: tree count differs")
    require(git(repo, "rev-parse", sha + "^{tree}").decode().strip() == data["tree"],
            f"{sha}: tree hash differs")
    commits = git(repo, "rev-list", "--reverse", sha,
                  *(["^" + previous] if previous else [])).decode().splitlines()
    require(commits == [row["sha"] for row in data["commits"]], f"{sha}: commits differ")
    if previous:
        fields = git(repo, "diff", "--name-status", "--no-renames", "-z", previous, sha)
        fields = fields.decode().split("\0")[:-1]
        changes = dict(zip(fields[1::2], fields[::2]))
        prior = tree(repo, previous)
    else:
        changes, prior = {name: "A" for name in entries}, {}
    require(changes == {row["path"]: row["change"] for row in data["files"]},
            f"{sha}: changed paths differ")
    for row in data["files"]:
        actual = (prior if row["change"] == "D" else entries)[row["path"]]
        require(actual == (row["mode"], row["kind"], row["blob"]),
                f"{sha}:{row['path']}: immutable Git metadata differs")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--go-source", type=Path)
    parser.add_argument("--rust-source", type=Path)
    parser.add_argument("--require-closed", action="store_true")
    args = parser.parse_args()
    require(bool(args.go_source) == bool(args.rust_source), "supply both source paths or neither")
    instructions = (INVENTORY / "implementation.md").read_text(encoding="utf-8")
    for packet in PACKETS | {"R01"}:
        require(f"## {packet} " in instructions, f"missing instructions for {packet}")
    total_files = total_commits = open_files = open_commits = 0
    for index, (sha, commits, files) in enumerate(EXPECTED, 1):
        data = json.loads((INVENTORY / f"s{index}-files.json").read_text(encoding="utf-8"))
        require(data["source"] == f"S{index}" and data["sha"] == sha, "source identity differs")
        require(len(data["commits"]) == commits and len(data["files"]) == files,
                f"S{index}: record counts differ")
        require(len({row["path"] for row in data["files"]}) == files, "duplicate path")
        for row in data["files"]:
            label = f"S{index}:{row['path']}"
            require(safe_path(row["path"]), f"{label}: unsafe source path")
            require(row["packet"] in PACKETS, f"{label}: unknown packet")
            require(row["status"] in STATES, f"{label}: unknown status")
            require(re.fullmatch(r"[0-9a-f]{40}", row["blob"]), f"{label}: invalid blob")
            for target in row["target_paths"]:
                require(safe_path(target) and (ROOT / target).exists(),
                        f"{label}: target does not exist: {target}")
            if row["status"] in CLOSED:
                require(row["evidence"], f"{label}: closed without evidence")
                if row["status"] in {"implemented", "equivalent"}:
                    require(row["target_paths"], f"{label}: closed without targets")
                if row["status"] == "implemented":
                    require(re.fullmatch(r"[0-9a-f]{40}", row.get("merged_commit", "")),
                            f"{label}: implemented without merged commit")
            else:
                open_files += 1
        for row in data["commits"]:
            require(row["status"] in STATES, f"{row['sha']}: unknown status")
            if row["status"] in CLOSED:
                require(row.get("evidence"), f"{row['sha']}: closed without evidence")
            else:
                open_commits += 1
        surfaces = json.loads((INVENTORY / f"s{index}-surfaces.json").read_text(encoding="utf-8"))
        paths = {row["path"]: row["packet"] for row in data["files"]}
        for surface in surfaces:
            require(paths.get(surface["path"]) == surface["packet"], "orphan surface")
        if args.go_source:
            verify_source(data, args.rust_source if index == 5 else args.go_source)
        total_files += files
        total_commits += commits
    print(f"Inventory valid: {total_commits} commits, {total_files} file-change records.")
    print(f"Still open: {open_commits} commits, {open_files} file-change records.")
    print("Git objects verified." if args.go_source else "Git objects not checked (no source paths).")
    if args.require_closed:
        require(not open_files and not open_commits, "absorption is incomplete")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"Absorption check failed: {error}", file=sys.stderr)
        sys.exit(1)
