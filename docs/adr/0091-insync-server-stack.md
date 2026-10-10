# ADR-0091: InSync server and client network stack

- Status: accepted
- Date: 2026-10-11
- Depends on: ADR-0089, ADR-0030

## Decision

### Server: single modular axum + SQLite service

`insync-server` is one Rust binary using:

- `axum` (with its WebSocket support) for REST + realtime on one port.
- `rusqlite` with the bundled SQLite (synchronous, wrapped in
  `spawn_blocking`-style serialized access) as the persistent store. SQLite in
  WAL mode is a reliable, single-file, easily backed-up database that fits a
  two-user product and needs no separate database service.
- `argon2` (PHC string encoding) for password hashing; bearer tokens are
  random 256-bit values stored hashed (SHA-256) with an expiry, revocable.
- In-process event fan-out to WebSocket connections plus a persisted
  `events` table (monotonic `seq` per relation) so clients catch up after
  disconnects with `GET /v1/events?after=<seq>`.
- Idempotency: every client-created object carries a client-generated
  `client_id` (UUIDv7); unique indexes `(relation_id, client_id)` make retries
  safe on messages, letters, pokes, todos, notes, anniversaries and focus
  transitions.
- Poke events and presence rows get retention pruning (default 7 days);
  messages/letters/shared data persist until relationship dissolution (then
  access is revoked immediately and data is purged after a 30-day grace run by
  a maintenance task).
- Deployment: one container image (or bare binary) + SQLite file + env vars;
  TLS terminates at a reverse proxy documented in `docs/insync/deployment.md`.

### Client: dedicated sync thread with its own async runtime

`insync-client` runs a `tokio` current-thread runtime on one dedicated OS
thread. It owns the WebSocket connection (`tokio-tungstenite`, rustls),
REST calls (`reqwest`, rustls, no default TLS), the outbox queue, reconnect
backoff and resync logic. It talks to the app through bounded std channels and
a snapshot cell; UI threads never block on the network. Pokes are local-first:
the local pet reaction fires immediately, the network send is queued.

### Shared protocol

`insync-core` defines the typed protocol (events, REST DTOs, error codes)
consumed by both server and client, so the wire format cannot drift.

### Time

Timestamps on the wire are Unix epoch milliseconds (i64). Client-side
calendars, alarms and timezone/DST handling use `jiff`, which is maintained,
supports the IANA database and civil-time arithmetic correctly. Local alarms
never depend on the network or server time.

### Dependency notes

- `axum`, `tokio`, `tower`, `hyper` — the standard Rust HTTP stack, exact-pinned.
- `rusqlite` — `bundled` feature compiles SQLite from source; no system library
  requirement, deterministic across platforms.
- `argon2`, `sha2`, `rand`, `uuid` (v7 feature) — RustCrypto/standard utility
  crates, exact-pinned.
- `reqwest` and `tokio-tungstenite` with rustls only (no native-tls).
- `jiff` for calendar/timezone correctness in clocks and alarms.
- Replacement boundary: any of these can be swapped behind `insync-core`
  traits; no third-party type leaks into InSync's public API surfaces except
  through the client/server crates themselves.

## Consequences

- One server process, one database file, one realtime channel — matching the
  deployment constraints (VPS/container, no clusters).
- The desktop app gains a tokio runtime thread; GPUI and tokio never share a
  reactor, and the GPUI executor stays free of blocking work.
- SQLite write concurrency is single-writer by design; the two-user load makes
  this a non-issue and keeps transactional semantics simple.
