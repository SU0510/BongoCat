//! REST request/response DTOs. Every endpoint under `/v1` uses these types.

use crate::{
    CoupleRelation, FocusSession, Letter, Message, Presence, Priority, PublicUser,
    SharedAnniversary, SharedNote, SharedTodo, UnixMs,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthResponse {
    pub user: PublicUser,
    pub token: String,
    pub expires_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenResponse {
    pub token: String,
    pub expires_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeResponse {
    pub user: PublicUser,
    pub relation: Option<CoupleRelation>,
    pub partner: Option<PublicUser>,
    pub presence: Option<Presence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InviteResponse {
    pub code: String,
    pub expires_at: UnixMs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptInviteRequest {
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairResponse {
    pub relation: CoupleRelation,
    pub partner: PublicUser,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendMessageRequest {
    pub client_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePage {
    pub messages: Vec<Message>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadRequest {
    pub last_message_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendLetterRequest {
    pub client_id: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendPokeRequest {
    pub client_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimited {
    pub retry_after_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusInviteRequest {
    pub client_id: String,
    pub topic: Option<String>,
    pub duration_secs: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTodoRequest {
    pub client_id: String,
    pub title: String,
    pub detail: Option<String>,
    pub priority: Priority,
    pub due_at: Option<UnixMs>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateTodoRequest {
    pub title: Option<String>,
    pub detail: Option<String>,
    pub priority: Option<Priority>,
    pub due_at: Option<UnixMs>,
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub client_id: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub content: String,
    pub base_revision: u64,
}

/// Returned with 409 when a note patch hits a stale revision so the user can
/// decide; nothing is dropped silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteConflict {
    pub current: SharedNote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAnniversaryRequest {
    pub client_id: String,
    pub name: String,
    pub date: String,
    pub repeat_yearly: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAnniversaryRequest {
    pub name: Option<String>,
    pub date: Option<String>,
    pub repeat_yearly: Option<bool>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceRequest {
    pub state: crate::PresenceState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverviewResponse {
    pub overview: crate::Overview,
}

/// Cursor page over the relation's event log for offline catch-up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventPage {
    pub events: Vec<crate::EventFrame>,
    pub latest_seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LetterPage {
    pub letters: Vec<Letter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoPage {
    pub todos: Vec<SharedTodo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotePage {
    pub notes: Vec<SharedNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnniversaryPage {
    pub anniversaries: Vec<SharedAnniversary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusResponse {
    pub focus: FocusSession,
}

/// Uniform error body: `{"error": {"code": "...", "message": "..."}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: String,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            error: ErrorDetail {
                code: code.to_string(),
                message: message.into(),
            },
        }
    }
}

/// Stable machine-readable error codes clients branch on.
pub mod error_codes {
    pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
    pub const FORBIDDEN: &str = "FORBIDDEN";
    pub const RELATION_INACTIVE: &str = "RELATION_INACTIVE";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const CONFLICT: &str = "CONFLICT";
    pub const RATE_LIMITED: &str = "RATE_LIMITED";
    pub const VALIDATION: &str = "VALIDATION";
    pub const USERNAME_TAKEN: &str = "USERNAME_TAKEN";
    pub const BAD_CREDENTIALS: &str = "BAD_CREDENTIALS";
    pub const INVITE_INVALID: &str = "INVITE_INVALID";
    pub const ALREADY_PAIRED: &str = "ALREADY_PAIRED";
    pub const TOKEN_EXPIRED: &str = "TOKEN_EXPIRED";
}
