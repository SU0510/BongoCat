# ADR-0089: InSync product pivot

- Status: accepted
- Date: 2026-10-11
- Depends on: ADR-0030 (decision ladder)

## Decision

Transform the product surface of this repository into **InSync**, a two-person
desktop companionship app for couples (pixel desk pet + realtime partner
interaction + shared daily tools), while keeping the reusable desktop
infrastructure of the BongoCat refactor intact.

### What InSync keeps unchanged

- `bongocat-storage` private-file primitives (atomic writes, 0600/0700 modes).
- `bongocat-log` sanitized logging with retention bounds.
- `bongocat-audio` rodio playback service (alarm and interaction sounds).
- `bongocat-i18n` localization infrastructure and locale conventions.
- `bongocat-platform` tray/muda menus, autostart, single-instance, theme,
  display bounds, clipboard.
- `gpui-kit` as the only settings/panel UI toolkit, following the existing
  window lifecycle patterns (`Root`, `Theme`, typed commands, snapshots).
- The versioned-JSON local store pattern (schema gate, atomic commit, backups,
  writer flock) proven by `bongocat-config`.
- Apache-2.0 licensing for all InSync code.

### What InSync does not use

- The Live2D/Cubism stack (`bongocat-live2d*`, `bongocat-model*`, `vendor/`)
  stays in the tree for the BongoCat app but is not part of InSync's dependency
  graph. See ADR-0090 for the replacement renderer.
- Global input monitoring (`bongocat-input`, Raw Input, CGEventTap). InSync has
  no reason to observe global keyboard/mouse/gamepad input, and the privacy
  requirements forbid uploading input content. InSync's pet window listens only
  to events on its own window.
- The updater/signing pipeline (`bongocat-update`, `bongocat-packaging` release
  flows) is not wired into InSync in this phase; InSync is delivered as a
  binary plus server deployment, not through BongoCat's update channel.

### New crates

- `insync-core`: shared domain model, wire protocol, offline queue and sync
  semantics (pure Rust, `forbid(unsafe_code)`, heavily unit tested).
- `insync-server`: standalone axum + SQLite backend (auth, pairing, chat,
  letters, pokes, focus sessions, shared todos/notes/anniversaries, presence,
  WebSocket events). Deploys on a VPS/container; never bundled into the app.
- `insync-client`: REST + WebSocket client and the sync engine (outbox,
  reconnect, resync, dedupe) used by the desktop app.
- `insync-pet-window`: the desk-pet window (per-platform, unsafe confined to
  the platform boundary, see ADR-0090).
- `insync-sprite`: sprite-sheet animation player and pet behavior state machine.
- `insync-app`: the InSync desktop binary (pet window + GPUI panels + tray).

### Data isolation

InSync stores its data under its own application root
(`com.ayangweb.insync/{development,production}/...`) using the same directory
discipline and atomic-write primitives as BongoCat. InSync never reads or writes
BongoCat's `config.json`, window state, or model store, and BongoCat builds keep
working untouched. The bundle identifier rule of ADR-0012 therefore applies per
product: BongoCat keeps `com.ayangweb.bongo-cat`; InSync uses
`com.ayangweb.insync`.

### Consequences

- The BongoCat app and its smoke modes keep compiling inside the workspace; the
  two products share infrastructure crates but have disjoint binaries, data
  roots and dependency leaves.
- Public user-facing docs stay BongoCat-scoped; InSync ships its own docs under
  `docs/insync/`.
