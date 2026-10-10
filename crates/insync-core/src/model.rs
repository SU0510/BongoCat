//! Persistent entities shared between the server store and the client caches.

use crate::{FocusSession, ProtocolError, UnixMs, limits, validate_body};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceState {
    Online,
    Away,
    Dnd,
}

/// The public view of a user. Passwords and tokens never appear here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub created_at: UnixMs,
}

/// One of the two sides of a relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationSide {
    Initiator,
    Partner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationStatus {
    Active,
    Dissolved,
}

/// The couple relation. Exactly two members; a user has at most one active
/// relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoupleRelation {
    pub id: String,
    pub user_a: String,
    pub user_b: String,
    pub status: RelationStatus,
    pub created_at: UnixMs,
    pub last_interaction_at: UnixMs,
    pub dissolved_at: Option<UnixMs>,
}

impl CoupleRelation {
    /// The other member, or `None` when `user_id` is not part of this
    /// relation. Authorization is always resolved through this check.
    pub fn partner_of(&self, user_id: &str) -> Option<&str> {
        if user_id == self.user_a {
            Some(&self.user_b)
        } else if user_id == self.user_b {
            Some(&self.user_a)
        } else {
            None
        }
    }

    pub fn member(&self, user_id: &str) -> bool {
        user_id == self.user_a || user_id == self.user_b
    }

    pub fn is_active(&self) -> bool {
        self.status == RelationStatus::Active
    }

    /// Side of `user_id`; `None` for non-members.
    pub fn side_of(&self, user_id: &str) -> Option<RelationSide> {
        if user_id == self.user_a {
            Some(RelationSide::Initiator)
        } else if user_id == self.user_b {
            Some(RelationSide::Partner)
        } else {
            None
        }
    }
}

/// A realtime chat message. `seq` is monotonic per relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub seq: u64,
    pub id: String,
    pub sender_id: String,
    pub body: String,
    pub created_at: UnixMs,
}

impl Message {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_body(&self.body, limits::MESSAGE_BODY_MAX)
    }
}

/// Per-user read watermark over the message log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadMarker {
    pub last_message_seq: u64,
    pub updated_at: UnixMs,
}

/// An asynchronous letter. One sender, one reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Letter {
    pub id: String,
    pub sender_id: String,
    pub body: String,
    pub created_at: UnixMs,
    pub read_at: Option<UnixMs>,
}

impl Letter {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_body(&self.body, limits::LETTER_BODY_MAX)
    }
}

/// A poke interaction record (retention-limited).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PokeRecord {
    pub id: String,
    pub sender_id: String,
    pub created_at: UnixMs,
}

/// A shared todo item. Personal todos never go on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedTodo {
    pub id: String,
    pub creator_id: String,
    pub title: String,
    pub detail: Option<String>,
    pub priority: Priority,
    pub due_at: Option<UnixMs>,
    pub completed_at: Option<UnixMs>,
    pub completed_by: Option<String>,
    pub created_at: UnixMs,
    pub updated_at: UnixMs,
    pub deleted_at: Option<UnixMs>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    Low,
    Normal,
    High,
}

impl SharedTodo {
    pub fn is_open(&self) -> bool {
        self.completed_at.is_none() && self.deleted_at.is_none()
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.title.trim().is_empty() || self.title.chars().count() > limits::TITLE_MAX {
            return Err(ProtocolError("todo title invalid".into()));
        }
        Ok(())
    }
}

/// A shared note. `revision` increases with every accepted update; a patch
/// against a stale revision is rejected so no content is silently lost.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedNote {
    pub id: String,
    pub author_id: String,
    pub content: String,
    pub revision: u64,
    pub updated_by: String,
    pub updated_at: UnixMs,
    pub created_at: UnixMs,
    pub deleted_at: Option<UnixMs>,
}

impl SharedNote {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_body(&self.content, limits::NOTE_BODY_MAX)
    }
}

/// A shared anniversary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedAnniversary {
    pub id: String,
    pub creator_id: String,
    pub name: String,
    /// Civil calendar date in the couple's shared display, `YYYY-MM-DD`.
    pub date: String,
    pub repeat_yearly: bool,
    pub note: Option<String>,
    pub created_at: UnixMs,
    pub updated_at: UnixMs,
    pub deleted_at: Option<UnixMs>,
}

impl SharedAnniversary {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.name.trim().is_empty() || self.name.chars().count() > limits::TITLE_MAX {
            return Err(ProtocolError("anniversary name invalid".into()));
        }
        let parts: Vec<_> = self.date.split('-').collect();
        let ok = parts.len() == 3
            && parts[0].len() == 4
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        if !ok {
            return Err(ProtocolError("anniversary date must be YYYY-MM-DD".into()));
        }
        Ok(())
    }
}

/// What the server currently knows about a user's availability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub user_id: String,
    pub state: PresenceState,
    pub online: bool,
    pub last_seen_at: UnixMs,
    pub focus_session_id: Option<String>,
}

/// A lightweight overview the client fetches once after (re)connecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overview {
    pub user: PublicUser,
    pub relation: Option<CoupleRelation>,
    pub partner: Option<PublicUser>,
    pub partner_presence: Option<Presence>,
    pub unread_messages: u64,
    pub last_message: Option<Message>,
    pub my_read_seq: u64,
    pub unread_letters: u64,
    pub recent_pokes: u64,
    pub active_focus: Option<FocusSession>,
}
