//! The event envelope and typed payloads pushed over the WebSocket and
//! persisted in the server's event log.

use crate::{
    FocusSession, Letter, Message, Presence, SharedAnniversary, SharedNote, SharedTodo, UnixMs,
};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    #[serde(rename = "message.created")]
    MessageCreated,
    #[serde(rename = "message.read")]
    MessageRead,
    #[serde(rename = "letter.created")]
    LetterCreated,
    #[serde(rename = "letter.read")]
    LetterRead,
    #[serde(rename = "poke.received")]
    PokeReceived,
    #[serde(rename = "focus.invited")]
    FocusInvited,
    #[serde(rename = "focus.accepted")]
    FocusAccepted,
    #[serde(rename = "focus.declined")]
    FocusDeclined,
    #[serde(rename = "focus.cancelled")]
    FocusCancelled,
    #[serde(rename = "focus.started")]
    FocusStarted,
    #[serde(rename = "focus.paused")]
    FocusPaused,
    #[serde(rename = "focus.resumed")]
    FocusResumed,
    #[serde(rename = "focus.completed")]
    FocusCompleted,
    #[serde(rename = "todo.created")]
    TodoCreated,
    #[serde(rename = "todo.updated")]
    TodoUpdated,
    #[serde(rename = "todo.completed")]
    TodoCompleted,
    #[serde(rename = "todo.deleted")]
    TodoDeleted,
    #[serde(rename = "note.created")]
    NoteCreated,
    #[serde(rename = "note.updated")]
    NoteUpdated,
    #[serde(rename = "note.deleted")]
    NoteDeleted,
    #[serde(rename = "anniversary.created")]
    AnniversaryCreated,
    #[serde(rename = "anniversary.updated")]
    AnniversaryUpdated,
    #[serde(rename = "anniversary.deleted")]
    AnniversaryDeleted,
    #[serde(rename = "presence.updated")]
    PresenceUpdated,
    #[serde(rename = "relation.dissolved")]
    RelationDissolved,
    #[serde(rename = "invite.accepted")]
    InviteAccepted,
}

impl EventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::MessageCreated => "message.created",
            EventKind::MessageRead => "message.read",
            EventKind::LetterCreated => "letter.created",
            EventKind::LetterRead => "letter.read",
            EventKind::PokeReceived => "poke.received",
            EventKind::FocusInvited => "focus.invited",
            EventKind::FocusAccepted => "focus.accepted",
            EventKind::FocusDeclined => "focus.declined",
            EventKind::FocusCancelled => "focus.cancelled",
            EventKind::FocusStarted => "focus.started",
            EventKind::FocusPaused => "focus.paused",
            EventKind::FocusResumed => "focus.resumed",
            EventKind::FocusCompleted => "focus.completed",
            EventKind::TodoCreated => "todo.created",
            EventKind::TodoUpdated => "todo.updated",
            EventKind::TodoCompleted => "todo.completed",
            EventKind::TodoDeleted => "todo.deleted",
            EventKind::NoteCreated => "note.created",
            EventKind::NoteUpdated => "note.updated",
            EventKind::NoteDeleted => "note.deleted",
            EventKind::AnniversaryCreated => "anniversary.created",
            EventKind::AnniversaryUpdated => "anniversary.updated",
            EventKind::AnniversaryDeleted => "anniversary.deleted",
            EventKind::PresenceUpdated => "presence.updated",
            EventKind::RelationDissolved => "relation.dissolved",
            EventKind::InviteAccepted => "invite.accepted",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "message.created" => EventKind::MessageCreated,
            "message.read" => EventKind::MessageRead,
            "letter.created" => EventKind::LetterCreated,
            "letter.read" => EventKind::LetterRead,
            "poke.received" => EventKind::PokeReceived,
            "focus.invited" => EventKind::FocusInvited,
            "focus.accepted" => EventKind::FocusAccepted,
            "focus.declined" => EventKind::FocusDeclined,
            "focus.cancelled" => EventKind::FocusCancelled,
            "focus.started" => EventKind::FocusStarted,
            "focus.paused" => EventKind::FocusPaused,
            "focus.resumed" => EventKind::FocusResumed,
            "focus.completed" => EventKind::FocusCompleted,
            "todo.created" => EventKind::TodoCreated,
            "todo.updated" => EventKind::TodoUpdated,
            "todo.completed" => EventKind::TodoCompleted,
            "todo.deleted" => EventKind::TodoDeleted,
            "note.created" => EventKind::NoteCreated,
            "note.updated" => EventKind::NoteUpdated,
            "note.deleted" => EventKind::NoteDeleted,
            "anniversary.created" => EventKind::AnniversaryCreated,
            "anniversary.updated" => EventKind::AnniversaryUpdated,
            "anniversary.deleted" => EventKind::AnniversaryDeleted,
            "presence.updated" => EventKind::PresenceUpdated,
            "relation.dissolved" => EventKind::RelationDissolved,
            "invite.accepted" => EventKind::InviteAccepted,
            _ => return None,
        })
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Payload carried by an [`EventFrame`]. Exactly one variant matches the
/// frame kind; the pairing is validated on construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventData {
    Message(Message),
    ReadSeq { last_message_seq: u64 },
    Letter(Letter),
    LetterId { letter_id: String },
    Poke { poke_id: String },
    Focus(Box<FocusSession>),
    FocusId { focus_id: String },
    Todo(Box<SharedTodo>),
    TodoId { todo_id: String },
    Note(Box<SharedNote>),
    NoteId { note_id: String },
    Anniversary(Box<SharedAnniversary>),
    AnniversaryId { anniversary_id: String },
    Presence(Box<Presence>),
    RelationId { relation_id: String },
    PartnerId { partner_id: String },
    Empty {},
}

impl Default for EventData {
    fn default() -> Self {
        EventData::Empty {}
    }
}

/// The wire envelope for every realtime event. `seq` is monotonic per
/// relation and persisted before delivery, so catch-up is always possible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventFrame {
    pub seq: u64,
    pub id: String,
    #[serde(rename = "type")]
    pub kind: EventKind,
    pub actor_id: String,
    pub relation_id: String,
    pub server_time: UnixMs,
    pub version: u32,
    pub data: EventData,
}

impl EventFrame {
    pub fn new(
        seq: u64,
        id: String,
        kind: EventKind,
        actor_id: String,
        relation_id: String,
        server_time: UnixMs,
        data: EventData,
    ) -> Self {
        Self {
            seq,
            id,
            kind,
            actor_id,
            relation_id,
            server_time,
            version: crate::PROTOCOL_VERSION,
            data,
        }
    }
}

/// A client -> server WebSocket frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientFrame {
    Hello { last_seq: Option<u64> },
    Ping,
}

/// A server -> client WebSocket frame (events plus control replies).
///
/// Serialized by hand: event frames go on the wire as the bare
/// [`EventFrame`] envelope (identical to the REST catch-up body so one
/// parser handles both), while control frames carry a `"type"` tag. A naive
/// internally-tagged enum would emit `type` twice for events and break
/// every client.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerFrame {
    Ready { latest_seq: u64 },
    Event(Box<EventFrame>),
    Pong,
    Error { code: String, message: String },
}

impl Serialize for ServerFrame {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ServerFrame::Event(frame) => frame.serialize(serializer),
            other => {
                #[derive(Serialize)]
                #[serde(tag = "type", rename_all = "snake_case")]
                enum Control<'a> {
                    Ready { latest_seq: u64 },
                    Pong,
                    Error { code: &'a str, message: &'a str },
                }
                match other {
                    ServerFrame::Ready { latest_seq } => Control::Ready {
                        latest_seq: *latest_seq,
                    }
                    .serialize(serializer),
                    ServerFrame::Pong => Control::Pong.serialize(serializer),
                    ServerFrame::Error { code, message } => {
                        Control::Error { code, message }.serialize(serializer)
                    }
                    ServerFrame::Event(_) => unreachable!("handled above"),
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for ServerFrame {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let tag = value.get("type").and_then(|t| t.as_str());
        match tag {
            Some("ready") => {
                let latest_seq = value
                    .get("latest_seq")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| serde::de::Error::custom("ready without latest_seq"))?;
                Ok(ServerFrame::Ready { latest_seq })
            }
            Some("pong") => Ok(ServerFrame::Pong),
            Some("error") => {
                let code = value
                    .get("code")
                    .and_then(|v| v.as_str())
                    .unwrap_or("UNKNOWN")
                    .to_string();
                let message = value
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(ServerFrame::Error { code, message })
            }
            _ => {
                // everything else must be an event envelope
                let frame: EventFrame =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(ServerFrame::Event(Box::new(frame)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_event_kind_strings() {
        for kind in [
            EventKind::MessageCreated,
            EventKind::PokeReceived,
            EventKind::FocusCompleted,
            EventKind::RelationDissolved,
        ] {
            assert_eq!(EventKind::parse(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_string(&kind).unwrap().trim_matches('"'),
                kind.as_str()
            );
        }
        assert_eq!(EventKind::parse("nope.nope"), None);
    }

    #[test]
    fn frame_json_shape_matches_protocol_doc() {
        let frame = EventFrame::new(
            128,
            "evt-1".into(),
            EventKind::MessageCreated,
            "u1".into(),
            "r1".into(),
            1_760_000_000_000,
            EventData::Message(Message {
                seq: 9,
                id: "m1".into(),
                sender_id: "u1".into(),
                body: "晚安".into(),
                created_at: 1_760_000_000_000,
            }),
        );
        let v: serde_json::Value = serde_json::to_value(&frame).unwrap();
        assert_eq!(v["type"], "message.created");
        assert_eq!(v["seq"], 128);
        assert_eq!(v["version"], 1);
        assert_eq!(v["data"]["body"], "晚安");
        let back: EventFrame = serde_json::from_value(v).unwrap();
        assert_eq!(back, frame);
    }

    #[test]
    fn client_and_server_frames_round_trip() {
        for f in [
            ClientFrame::Hello { last_seq: Some(7) },
            ClientFrame::Hello { last_seq: None },
            ClientFrame::Ping,
        ] {
            let json = serde_json::to_string(&f).unwrap();
            let back: ClientFrame = serde_json::from_str(&json).unwrap();
            assert_eq!(back, f);
        }
        for f in [
            ServerFrame::Ready { latest_seq: 42 },
            ServerFrame::Pong,
            ServerFrame::Error {
                code: "x".into(),
                message: "y".into(),
            },
        ] {
            let json = serde_json::to_string(&f).unwrap();
            let back: ServerFrame = serde_json::from_str(&json).unwrap();
            assert_eq!(back, f);
        }
    }

    #[test]
    fn unknown_event_type_is_rejected_not_guessed() {
        let json = r#"{"seq":1,"id":"a","type":"future.thing","actor_id":"u",
            "relation_id":"r","server_time":0,"version":1,"data":{}}"#;
        let parsed: Result<EventFrame, _> = serde_json::from_str(json);
        assert!(
            parsed.is_err(),
            "unknown kinds must fail parsing so the client skip logic sees raw text"
        );
    }
}
