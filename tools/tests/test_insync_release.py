"""Keep the InSync release channel honest: one version, one entry, one tag shape.

InSync ships on its own channel (ADR-0092): an `insync-v*` tag publishes the
binaries that `insync-verify` already built and tested for that commit, with
checksums and the changelog entry for the version. Everything here fails
*locally* the same way the release would fail after a tag was pushed — a crate
that disagrees about the version, a version with no changelog entry, a section
heading outside the shared vocabulary, a release workflow that could take over
BongoCat's channel or publish something no run verified.
"""

import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools" / "insync-release.py"
CHANGELOG = ROOT / "docs" / "insync" / "CHANGELOG.md"
CONVENTIONS = ROOT / "docs" / "changelog-conventions.md"
LOCKFILE = ROOT / "Cargo.lock"
RELEASE_WORKFLOW = ROOT / ".github" / "workflows" / "insync-release.yml"
VERIFY_WORKFLOW = ROOT / ".github" / "workflows" / "insync-verify.yml"

RELEASE_COMMIT_SUBJECT = "chore(insync): release v$version"
VARIATION_SELECTOR = "\ufe0f"


def run_tool(*args):
    return subprocess.run(
        [sys.executable, str(TOOL), *args],
        capture_output=True,
        text=True,
        cwd=ROOT,
    )


def strip_variation_selector(text: str) -> str:
    return text.replace(VARIATION_SELECTOR, "")


def vocabulary():
    """The `(emoji, english, chinese)` rows of the shared section vocabulary.

    Read from the conventions document rather than restated here, so a heading
    added to the vocabulary is legal for the InSync changelog without touching
    this test, and a heading invented in the changelog is not.
    """
    rows = []
    for line in CONVENTIONS.read_text(encoding="utf-8").splitlines():
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 3 or not cells[0]:
            continue
        if cells[0] in {"Emoji", "英文"} or set(cells[0]) <= {"-"}:
            continue
        rows.append(tuple(cells))
    return rows


def headings(path: Path, level: str):
    pattern = re.compile(rf"^{level} (?P<title>.+?)\s*$", re.MULTILINE)
    return [match.group("title") for match in pattern.finditer(path.read_text(encoding="utf-8"))]


def crate_manifests(root: Path):
    members = re.search(
        r"^members\s*=\s*\[(?P<body>.*?)\]",
        (root / "Cargo.toml").read_text(encoding="utf-8"),
        re.MULTILINE | re.DOTALL,
    )
    directories = [
        member
        for member in re.findall(r'"([^"]+)"', members.group("body"))
        if Path(member).name.startswith("insync-")
    ]
    return [root / directory / "Cargo.toml" for directory in directories]


def trigger_paths(text: str, marker: str):
    """The quoted path entries of one trigger block in the verify workflow.

    Read as text on purpose: these tests run in a job that installs nothing but
    Python, so they must not import a YAML parser.
    """
    block = text.split(marker, 1)[1]
    lines = []
    for line in block.splitlines():
        # `paths:` sits four spaces in; a key at two spaces or a dedent ends it.
        if line and not line.startswith("    "):
            break
        lines.append(line)
    return re.findall(r'^\s+- "([^"]+)"\s*$', "\n".join(lines), re.MULTILINE)


class VersionContractTests(unittest.TestCase):
    def test_the_crates_agree_on_one_release_version(self):
        result = run_tool("version")
        self.assertEqual(result.returncode, 0, result.stderr)
        version = result.stdout.strip()
        self.assertRegex(version, r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")

    def test_the_lockfile_records_the_same_version(self):
        version = run_tool("version").stdout.strip()
        source = LOCKFILE.read_text(encoding="utf-8")
        recorded = dict(
            re.findall(r'\[\[package\]\]\nname = "(insync-[^"]+)"\nversion = "([^"]+)"', source)
        )
        self.assertTrue(recorded, "Cargo.lock must contain the InSync crates")
        # `--locked` is used by every CI job, so a version bump that skipped
        # `cargo update --workspace` would fail every build instead of this test.
        self.assertEqual(set(recorded.values()), {version})

    def test_a_crate_that_disagrees_about_the_version_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy(ROOT / "Cargo.toml", root / "Cargo.toml")
            manifests = crate_manifests(ROOT)
            for manifest in manifests:
                target = root / manifest.relative_to(ROOT)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(manifest, target)
            (root / "docs" / "insync").mkdir(parents=True)
            shutil.copy(CHANGELOG, root / "docs" / "insync" / "CHANGELOG.md")

            drifted = root / manifests[0].relative_to(ROOT)
            drifted.write_text(
                re.sub(r'^version\s*=\s*"[^"]+"', 'version = "9.9.9"', drifted.read_text(encoding="utf-8"), count=1, flags=re.MULTILINE),
                encoding="utf-8",
            )

            result = run_tool("version", "--root", str(root))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("disagree", result.stderr)
            self.assertIn("9.9.9", result.stderr)

    def test_a_crate_that_still_inherits_the_bongo_version_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copy(ROOT / "Cargo.toml", root / "Cargo.toml")
            manifests = crate_manifests(ROOT)
            for manifest in manifests:
                target = root / manifest.relative_to(ROOT)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(manifest, target)

            inherited = root / manifests[0].relative_to(ROOT)
            inherited.write_text(
                re.sub(
                    r'^version\s*=\s*"[^"]+"',
                    "version.workspace = true",
                    inherited.read_text(encoding="utf-8"),
                    count=1,
                    flags=re.MULTILINE,
                ),
                encoding="utf-8",
            )

            result = run_tool("version", "--root", str(root))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("explicit version", result.stderr)


class ChangelogContractTests(unittest.TestCase):
    """The InSync changelog's shape.

    Whether an entry exists for the version being tagged is enforced where it
    matters, by the release workflow fetching it before anything is published;
    these tests keep the file in a state where that lookup can only succeed or
    fail for the one real reason (the entry was never written).
    """

    def current_version(self):
        result = run_tool("version")
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout.strip()

    def versioned_entries(self):
        return [
            heading
            for heading in headings(CHANGELOG, "##")
            if re.match(r"^\d+\.\d+\.\d+(?:\s|$)", heading)
        ]

    def test_the_newest_heading_is_unreleased_or_the_version_being_released(self):
        top = headings(CHANGELOG, "##")[0]
        if top == "Unreleased":
            return
        version = self.current_version()
        self.assertTrue(
            top.startswith(version),
            f"the newest changelog entry is '## {top}' but the crates say {version}: "
            "a version entry is renamed in the release commit, so the newest one can "
            "only be the version being released",
        )

    def test_every_versioned_entry_is_dated(self):
        # A release commit renames `## Unreleased` to `## <version> - <date>`; an
        # entry without a date never went through that step.
        for heading in self.versioned_entries():
            with self.subTest(heading=heading):
                self.assertRegex(heading, r"^\d+\.\d+\.\d+ - \d{4}-\d{2}-\d{2}$")

    def test_every_section_heading_comes_from_the_shared_vocabulary(self):
        # The vocabulary is shared with the two root changelogs so a reader of
        # either product meets the same sections; the InSync file uses the
        # Chinese column, like the rest of docs/insync.
        allowed = {
            strip_variation_selector(emoji): chinese for emoji, _, chinese in vocabulary()
        }
        self.assertTrue(allowed, "the conventions document must publish a vocabulary")

        for heading in headings(CHANGELOG, "###"):
            with self.subTest(heading=heading):
                parts = heading.split(" ", 1)
                self.assertEqual(len(parts), 2, "a section heading is an emoji plus its text")
                emoji, text = strip_variation_selector(parts[0]), parts[1]
                self.assertIn(emoji, allowed, f"{emoji!r} is not in the shared vocabulary")
                self.assertEqual(text, allowed[emoji])

    def test_the_reader_returns_one_entry_and_stops_at_the_next(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs" / "insync").mkdir(parents=True)
            (root / "docs" / "insync" / "CHANGELOG.md").write_text(
                "# 更新日志\n\n"
                "## Unreleased\n\n### ✨ 新功能\n\n- 还没发布的改动。\n\n"
                "## 1.2.3 - 2026-10-11\n\n### 🐛 问题修复\n\n- 这是 1.2.3 的说明。\n\n"
                "## 1.2.2 - 2026-10-10\n\n### ✨ 新功能\n\n- 旧版本的说明。\n",
                encoding="utf-8",
            )
            result = run_tool("notes", "1.2.3", "--root", str(root))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("这是 1.2.3 的说明。", result.stdout)
            self.assertNotIn("旧版本的说明。", result.stdout)
            self.assertNotIn("还没发布的改动。", result.stdout)

    def test_a_version_without_an_entry_cannot_be_released(self):
        result = run_tool("notes", "99.99.99")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("99.99.99", result.stderr)

    def test_an_empty_entry_cannot_be_released(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs" / "insync").mkdir(parents=True)
            (root / "docs" / "insync" / "CHANGELOG.md").write_text(
                "# 更新日志\n\n## 1.0.0 - 2026-10-11\n\n## 1.0.1 - 2026-10-12\n\n- 说明。\n",
                encoding="utf-8",
            )
            result = run_tool("notes", "1.0.0", "--root", str(root))
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("empty", result.stderr)


class ReleaseWorkflowContractTests(unittest.TestCase):
    """The release's promises, checked as the workflow is written."""

    def workflow(self):
        return RELEASE_WORKFLOW.read_text(encoding="utf-8")

    def test_it_fires_only_for_the_insync_tag_namespace(self):
        text = self.workflow()
        self.assertIn('"insync-v*"', text)
        # A plain `v*` filter would make this workflow a second claimant on
        # BongoCat's tag channel, which packages installers and an update
        # manifest this one must never touch.
        self.assertNotIn('- "v*"', text)

    def test_it_refuses_a_tag_that_does_not_match_the_version(self):
        text = self.workflow()
        self.assertIn("insync-v$version", text)
        self.assertIn("tools/insync-release.py version", text)

    def test_it_refuses_a_commit_that_is_not_a_release_commit(self):
        text = self.workflow()
        # The subject the workflow compares against is the documented one: a
        # release commit is titled `chore(insync): release v<version>`, the
        # InSync counterpart of BongoCat's `chore: release v<version>`.
        self.assertIn(RELEASE_COMMIT_SUBJECT, text)
        self.assertIn("git log -1", text)

    def test_it_publishes_only_artifacts_a_green_verify_run_produced(self):
        text = self.workflow()
        self.assertIn("insync-verify.yml/runs?head_sha=", text)
        self.assertIn('conclusion == "success"', text)
        self.assertIn("gh run download", text)
        # The binaries must come from that run, not from a fresh build.
        self.assertNotIn("cargo build", text)

    def test_it_attaches_checksums_and_changelog_notes(self):
        text = self.workflow()
        self.assertIn("SHA256SUMS", text)
        self.assertIn("sha256sum", text)
        self.assertIn('insync-release.py notes "$version"', text)
        self.assertIn("--notes-file", text)
        self.assertIn("--verify-tag", text)

    def test_every_asset_name_it_uploads_is_named_for_its_platform(self):
        text = self.workflow()
        for asset in (
            "insync-app-macos-aarch64",
            "insync-server-macos-aarch64",
            "insync-app-windows-x86_64.exe",
            "insync-server-windows-x86_64.exe",
        ):
            with self.subTest(asset=asset):
                self.assertIn(asset, text)

    def test_the_verify_workflow_runs_these_contracts_on_every_push(self):
        text = VERIFY_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("python3 -m unittest discover -s tools/tests -p 'test_insync_*.py'", text)
        for path in (
            '"tools/insync-release.py"',
            '"tools/tests/test_insync_*.py"',
            '".github/workflows/insync-release.yml"',
        ):
            with self.subTest(path=path):
                self.assertIn(path, text)

    def test_push_and_pull_request_watch_the_same_paths(self):
        # A pull request that changes a build input must run the jobs a push
        # would; the two lists once disagreed (a Cargo edit in a PR ran nothing).
        text = VERIFY_WORKFLOW.read_text(encoding="utf-8")
        push = trigger_paths(text, "\n  push:")
        pull_request = trigger_paths(text, "\n  pull_request:")
        self.assertTrue(push, "the verify workflow must list push paths")
        self.assertEqual(push, pull_request)
        # The release notes come from this file by tag, and its headings are
        # checked here, so editing it is a build input.
        self.assertIn("docs/insync/CHANGELOG.md", push)

    def test_the_release_commit_subject_is_the_documented_one(self):
        # The workflow compares against this exact string, so the documented
        # subject and the enforced one are the same string by construction.
        self.assertIn(RELEASE_COMMIT_SUBJECT, self.workflow())


if __name__ == "__main__":
    unittest.main()
