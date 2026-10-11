# ADR-0092: InSync versioning and release channel

- Status: accepted
- Date: 2026-10-11
- Depends on: ADR-0089, ADR-0091

## Context

InSync lives in this repository but is not BongoCat: different product, different
audience, different release cadence. BongoCat's channel is bound to the single
`[workspace.package].version` — one product version, matched by
`tools/tests/test_product_version_contract.py`, tagged `v<version>` and published
by `release.yml` as signed installers plus an update manifest.

An earlier InSync task note ("F2") resolved that the six `insync-*` crates should
inherit the workspace version so the existing contract test would pass. That made
the contract green but coupled the two products: every InSync iteration would
either need a BongoCat version bump or would leave the repository advertising a
product version that does not describe it, and the first InSync release would
have to be tagged `v2.2.0` — a BongoCat version number — while `release.yml`
would claim that tag and try to package installers for it.

## Decision

### Version number

- InSync carries its own version, starting at **0.1.0**. The `0.x` major is
  deliberate: local data formats, the wire protocol and per-platform behaviour may
  still change incompatibly, and the number should say so.
- All six `insync-*` crates declare that version **explicitly** (`version =
  "0.1.0"`, not `version.workspace = true`), so the value is visible in the crate
  that ships rather than inherited from a product it is not part of.
- `test_product_version_contract.py` now applies only to BongoCat members
  (its `insync-*` carve-out points here). InSync's own version contract lives in
  `tools/tests/test_insync_release.py`: the six crates must agree, the lockfile
  must record the same value, and a crate that disagrees — or that silently goes
  back to inheriting the workspace version — fails the test and the release.
- Version bumps follow SemVer with the usual `0.x` reading: breaking change on
  any position until 1.0.0. A bump touches the six manifests and `Cargo.lock`
  (`cargo update --workspace`) in one commit.

### Tag namespace

- InSync releases are tagged `insync-v<version>` (e.g. `insync-v0.1.0`).
- BongoCat's `release.yml` matches `v*` and `insync-v0.1.0` does **not** match
  `v*`, so the two channels cannot claim each other's tags. The InSync release
  workflow asserts its own namespace and the absence of a plain `v*` filter, so a
  future edit cannot quietly turn it into a second claimant on BongoCat's channel.

### Release content: publish what was verified, never rebuild

`insync-verify.yml` already builds and tests the release binaries for macOS
(aarch64) and Windows (x86_64) on every push to `feat/insync` / `master`.

`insync-release.yml` (triggered by the tag, with `workflow_dispatch` for a manual
re-run) therefore:

1. reads the version from the manifests and refuses a tag that disagrees;
2. refuses a commit whose subject is not `chore(insync): release v<version>`;
3. looks up the `insync-verify` run for the **tagged commit** and requires
   `conclusion == "success"`;
4. downloads *that run's* artifacts (`gh run download`) and renames them to
   platform-named assets — four files: `insync-app` / `insync-server` for
   `macos-aarch64` and `windows-x86_64.exe`;
5. writes `SHA256SUMS` over them;
6. composes the notes from `docs/insync/CHANGELOG.md` (the `## <version>` entry,
   by tag) plus a header stating what the artifacts are and are not;
7. creates the release with `--verify-tag --notes-file` and attaches the assets.

The workflow contains no `cargo build`: a released binary is by construction one a
green CI run built and tested for that exact commit, and the release notes link
that run. `tools/insync-release.py` is the single reader of the version and the
notes, used by both the workflow and the tests.

### What a release is not (still)

- No installers (`.app` bundle / `.dmg` / NSIS), no code signing, no notarization,
  no auto-update. InSync is not wired into `bongocat-packaging`; assets are bare
  binaries and the notes say so, including the Gatekeeper / SmartScreen
  consequences.
- No artifact leaves this repository behind an authorization gate: publishing a
  release is pushing a tag, and the assets are the fork's own CI output.

## Consequences

- The two products version and release independently; InSync can iterate without
  touching BongoCat's product version, and vice versa.
- Every published InSync binary has a verifiable provenance: tag → commit →
  verify run → artifact, with checksums in the release itself.
- A release can only be made from a commit that already passed CI, and the tag,
  the crate versions and the changelog entry must all agree — the failure modes
  that make a release dishonest are refused before anything is published.
- Releasing requires a pushed branch and a green run, so work cannot be "released"
  from a dirty local tree.
- Because assets are bare binaries, a user still has to sign/notarize locally on
  macOS (or accept the SmartScreen prompt on Windows); the notes state this
  instead of pretending the artifacts are installable.
