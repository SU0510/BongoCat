//! InSync shared domain model, wire protocol and offline semantics.
//!
//! This crate is the single source of truth for everything that crosses the
//! network or persists across restarts on the client: entities, the event
//! envelope, REST DTOs, the focus-session transition rules and the outbox.
//! The server and the desktop client both depend on it so the wire format
//! cannot drift. It performs no I/O and holds no platform types.

pub mod api;
pub mod event;
pub mod focus;
pub mod model;
pub mod outbox;
pub mod sync;

pub use api::*;
pub use event::*;
pub use focus::*;
pub use model::*;
pub use outbox::*;
pub use sync::*;

/// Wire protocol version carried by every event frame.
pub const PROTOCOL_VERSION: u32 = 1;

/// Server-defined bounds enforced on both sides.
pub mod limits {
    /// Maximum rendered message body length in characters.
    pub const MESSAGE_BODY_MAX: usize = 2000;
    /// Maximum letter body length in characters.
    pub const LETTER_BODY_MAX: usize = 4000;
    /// Maximum shared note body length in characters.
    pub const NOTE_BODY_MAX: usize = 8000;
    /// Maximum todo/note/anniversary title length in characters.
    pub const TITLE_MAX: usize = 200;
    /// Maximum focus topic length in characters.
    pub const FOCUS_TOPIC_MAX: usize = 200;
    /// Focus duration bounds in seconds (1 minute ..= 2 hours).
    pub const FOCUS_DURATION_MIN: i64 = 60;
    pub const FOCUS_DURATION_MAX: i64 = 7200;
    /// Unaccepted focus invites auto-cancel after this long.
    pub const FOCUS_INVITE_TIMEOUT_MS: i64 = 10 * 60 * 1000;
    /// Poke notifications expire server-side after this long.
    pub const POKE_RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;
    /// A pending poke that could not be delivered within this window is
    /// dropped by the client outbox instead of replayed stale.
    pub const POKE_OUTBOX_TTL_MS: i64 = 10 * 60 * 1000;
    /// Server cooldown between pokes from the same sender.
    pub const POKE_COOLDOWN_MS: i64 = 2000;
    /// Short-term interaction events the client treats as "recent".
    pub const RECENT_INTERACTION_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
}

/// Millisecond Unix timestamp used everywhere on the wire.
pub type UnixMs = i64;

/// Generates a client-side idempotency key (UUID v7).
pub fn new_client_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("protocol violation: {0}")]
pub struct ProtocolError(pub String);

/// Validates a body length against the protocol limits.
pub fn validate_body(body: &str, max: usize) -> Result<(), ProtocolError> {
    if body.chars().count() > max {
        return Err(ProtocolError(format!("body longer than {max} characters")));
    }
    if body.trim().is_empty() {
        return Err(ProtocolError("body must not be empty".into()));
    }
    Ok(())
}
