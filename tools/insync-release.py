#!/usr/bin/env python3
"""The single reader for the InSync version and its release notes.

InSync versions on its own channel (ADR-0092): its own SemVer, its own
`insync-v*` tag namespace, its own changelog entry per release. The contract
tests and the release workflow both read the version through this tool, so the
number cannot be spelled differently in the two places that have to agree.

Usage:
    tools/insync-release.py version [--root DIR]
    tools/insync-release.py notes <version> [--root DIR]

Both subcommands exit non-zero on a state that cannot be released: crates that
disagree about the version, a version that is not SemVer, or a version whose
changelog entry was never written.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CHANGELOG = Path("docs") / "insync" / "CHANGELOG.md"
VERSION_PATTERN = re.compile(r'^version\s*=\s*"([^"]+)"\s*$', re.MULTILINE)
SEMVER_PATTERN = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")


def insync_members(root: Path) -> list[str]:
    """Every InSync crate, read from the workspace so a new one cannot be forgotten."""
    manifest = (root / "Cargo.toml").read_text(encoding="utf-8")
    workspace = re.search(
        r"^\[workspace\]\s*\n(?P<body>.*?)(?=^\[|\Z)",
        manifest,
        re.MULTILINE | re.DOTALL,
    )
    if workspace is None:
        raise SystemExit("[workspace] is missing from Cargo.toml")

    members = re.search(
        r"^members\s*=\s*\[(?P<body>.*?)\]",
        workspace.group("body"),
        re.MULTILINE | re.DOTALL,
    )
    if members is None:
        raise SystemExit("[workspace].members is missing from Cargo.toml")

    found = [
        member
        for member in re.findall(r'"([^"]+)"', members.group("body"))
        if Path(member).name.startswith("insync-")
    ]
    if not found:
        raise SystemExit("[workspace].members lists no InSync crates")
    return found


def version(root: Path = ROOT) -> str:
    """The one version all InSync crates declare."""
    declared = {}
    for member in insync_members(root):
        manifest = root / member / "Cargo.toml"
        match = VERSION_PATTERN.search(manifest.read_text(encoding="utf-8"))
        if match is None:
            raise SystemExit(f"{member}/Cargo.toml declares no explicit version")
        declared[member] = match.group(1)

    distinct = set(declared.values())
    if len(distinct) != 1:
        detail = ", ".join(f"{member}={value}" for member, value in sorted(declared.items()))
        raise SystemExit(f"InSync crates disagree about the version: {detail}")

    only = distinct.pop()
    if not SEMVER_PATTERN.match(only):
        raise SystemExit(f"InSync version {only!r} is not a release version")
    return only


def notes(release: str, changelog: Path = CHANGELOG) -> str:
    """The changelog entry for one version, which is the release's own notes."""
    source = changelog.read_text(encoding="utf-8")
    heading = re.search(rf"^## {re.escape(release)}(?:\s|$).*$", source, re.MULTILINE)
    if heading is None:
        raise SystemExit(
            f"{changelog} has no '## {release}' entry; write it before tagging {release}"
        )

    rest = source[heading.end() :]
    following = re.search(r"^## ", rest, re.MULTILINE)
    body = (rest[: following.start()] if following else rest).strip()
    if not body:
        raise SystemExit(f"the '## {release}' entry in {changelog} is empty")
    return body


def main() -> int:
    args = sys.argv[1:]
    root = ROOT
    if "--root" in args:
        index = args.index("--root")
        root = Path(args[index + 1]).resolve()
        del args[index : index + 2]

    if not args:
        print(__doc__)
        return 2
    if args[0] == "version":
        print(version(root))
        return 0
    if args[0] == "notes":
        if len(args) < 2:
            print("usage: insync-release.py notes <version>", file=sys.stderr)
            return 2
        print(notes(args[1], root / CHANGELOG))
        return 0

    print(f"unknown subcommand {args[0]!r}", file=sys.stderr)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
