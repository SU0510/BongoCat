//! Chat: realtime messages, letters, pokes, the overview aggregate and the
//! event catch-up endpoint.

use crate::auth::AuthUser;
use crate::db::Db;
use crate::error::{ApiError, is_unique_violation};
use crate::events;
use crate::routes::{public_user, relation_for, require_active_relation, touch_relation};
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use insync_core::api::{
    EventPage, LetterPage, MessagePage, OverviewResponse, ReadRequest, SendLetterRequest,
    SendMessageRequest, SendPokeRequest,
};
use insync_core::{Letter, Message, Overview, PokeRecord, Presence, PresenceState, limits};
use rusqlite::params;
use std::collections::HashMap;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/messages", get(list_messages).post(send_message))
        .route("/messages/read", post(mark_read))
        .route("/letters", get(list_letters).post(send_letter))
        .route("/letters/{id}/read", post(read_letter))
        .route("/letters/{id}", delete_letter_route())
        .route("/pokes", post(send_poke))
        .route("/overview", get(overview))
        .route("/events", get(list_events))
}

fn delete_letter_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::delete(delete_letter)
}

// ---------------------------------------------------------------- messages

fn row_message(row: &rusqlite::Row<'_>) -> Result<Message, rusqlite::Error> {
    Ok(Message {
        seq: row.get::<_, i64>(0)? as u64,
        id: row.get(1)?,
        sender_id: row.get(2)?,
        body: row.get(3)?,
        created_at: row.get(4)?,
    })
}

async fn list_messages(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<MessagePage>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let after_seq: i64 = q.get("after_seq").and_then(|v| v.parse().ok()).unwrap_or(0);
    let limit: i64 = q
        .get("limit")
        .and_then(|v| v.parse().ok())
        .map(|l: i64| l.clamp(1, 200))
        .unwrap_or(100);
    let mut stmt = conn
        .prepare(
            "SELECT seq, id, sender_id, body, created_at FROM messages
             WHERE relation_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
        )
        .map_err(ApiError::from)?;
    let rows = stmt
        .query_map(params![relation.id, after_seq, limit + 1], row_message)
        .map_err(ApiError::from)?;
    let mut messages = Vec::new();
    for row in rows {
        messages.push(row.map_err(ApiError::from)?);
    }
    let has_more = messages.len() as i64 > limit;
    if has_more {
        messages.truncate(limit as usize);
    }
    Ok(Json(MessagePage { messages, has_more }))
}

async fn send_message(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<Message>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    insync_core::validate_body(&req.body, limits::MESSAGE_BODY_MAX)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    if let Err(retry) = state
        .rate
        .check(&format!("msg:{}", user.id), 20, 60 * 1000, Db::now_ms())
    {
        return Err(ApiError::rate_limited(retry));
    }
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    // per-relation seq allocation is serialized by the connection mutex
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM messages WHERE relation_id = ?1",
            [&relation.id],
            |row| row.get(0),
        )
        .map_err(ApiError::from)?;
    let id = uuid::Uuid::now_v7().to_string();
    match conn.execute(
        "INSERT INTO messages (id, relation_id, seq, sender_id, body, client_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, relation.id, seq, user.id, req.body, req.client_id, now],
    ) {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            // idempotent retry: return the already-stored message
            let existing = conn
                .query_row(
                    "SELECT seq, id, sender_id, body, created_at FROM messages
                     WHERE relation_id = ?1 AND client_id = ?2",
                    params![relation.id, req.client_id],
                    row_message,
                )
                .map_err(ApiError::from)?;
            return Ok((StatusCode::OK, Json(existing)));
        }
        Err(e) => return Err(e.into()),
    }
    touch_relation(&conn, &relation.id).map_err(ApiError::from)?;
    let message = Message {
        seq: seq as u64,
        id,
        sender_id: user.id.clone(),
        body: req.body,
        created_at: now,
    };
    events::emit(
        &state,
        &conn,
        &relation.id,
        insync_core::EventKind::MessageCreated,
        &user.id,
        insync_core::EventData::Message(message.clone()),
    )?;
    Ok((StatusCode::CREATED, Json(message)))
}

async fn mark_read(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<ReadRequest>,
) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    conn.execute(
        "INSERT INTO message_reads (relation_id, user_id, last_message_seq, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (relation_id, user_id)
         DO UPDATE SET last_message_seq = MAX(last_message_seq, excluded.last_message_seq),
                       updated_at = excluded.updated_at",
        params![
            relation.id,
            user.id,
            req.last_message_seq as i64,
            Db::now_ms()
        ],
    )
    .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        insync_core::EventKind::MessageRead,
        &user.id,
        insync_core::EventData::ReadSeq {
            last_message_seq: req.last_message_seq,
        },
    )?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- letters

fn row_letter(row: &rusqlite::Row<'_>) -> Result<Letter, rusqlite::Error> {
    Ok(Letter {
        id: row.get(0)?,
        sender_id: row.get(1)?,
        body: row.get(2)?,
        created_at: row.get(3)?,
        read_at: row.get(4)?,
    })
}

async fn list_letters(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<LetterPage>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let unread_only = q.get("unread").is_some_and(|v| v == "true");
    let sql = if unread_only {
        "SELECT id, sender_id, body, created_at, read_at FROM letters
         WHERE relation_id = ?1 AND sender_id != ?2 AND read_at IS NULL
         ORDER BY created_at DESC LIMIT 200"
    } else {
        "SELECT id, sender_id, body, created_at, read_at FROM letters
         WHERE relation_id = ?1
         ORDER BY created_at DESC LIMIT 200"
    };
    let mut stmt = conn.prepare(sql).map_err(ApiError::from)?;
    let rows = if unread_only {
        stmt.query_map(params![relation.id, user.id], row_letter)
            .map_err(ApiError::from)?
    } else {
        stmt.query_map(params![relation.id, user.id], row_letter)
            .map_err(ApiError::from)?
    };
    let mut letters = Vec::new();
    for row in rows {
        letters.push(row.map_err(ApiError::from)?);
    }
    Ok(Json(LetterPage { letters }))
}

async fn send_letter(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<SendLetterRequest>,
) -> Result<(StatusCode, Json<Letter>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    insync_core::validate_body(&req.body, limits::LETTER_BODY_MAX)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let id = uuid::Uuid::now_v7().to_string();
    match conn.execute(
        "INSERT INTO letters (id, relation_id, sender_id, body, client_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, relation.id, user.id, req.body, req.client_id, now],
    ) {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = conn
                .query_row(
                    "SELECT id, sender_id, body, created_at, read_at FROM letters
                     WHERE relation_id = ?1 AND client_id = ?2",
                    params![relation.id, req.client_id],
                    row_letter,
                )
                .map_err(ApiError::from)?;
            return Ok((StatusCode::OK, Json(existing)));
        }
        Err(e) => return Err(e.into()),
    }
    touch_relation(&conn, &relation.id).map_err(ApiError::from)?;
    let letter = Letter {
        id,
        sender_id: user.id.clone(),
        body: req.body,
        created_at: now,
        read_at: None,
    };
    events::emit(
        &state,
        &conn,
        &relation.id,
        insync_core::EventKind::LetterCreated,
        &user.id,
        insync_core::EventData::Letter(letter.clone()),
    )?;
    Ok((StatusCode::CREATED, Json(letter)))
}

async fn read_letter(
    State(state): State<AppState>,
    user: AuthUser,
    Path(letter_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let sender: String = conn
        .query_row(
            "SELECT sender_id FROM letters WHERE id = ?1 AND relation_id = ?2",
            params![letter_id, relation.id],
            |row| row.get(0),
        )
        .map_err(|_| ApiError::not_found("no such letter"))?;
    if sender == user.id {
        return Err(ApiError::validation("cannot read your own letter"));
    }
    let now = Db::now_ms();
    conn.execute(
        "UPDATE letters SET read_at = ?1 WHERE id = ?2 AND read_at IS NULL",
        params![now, letter_id],
    )
    .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        insync_core::EventKind::LetterRead,
        &user.id,
        insync_core::EventData::LetterId { letter_id },
    )?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_letter(
    State(state): State<AppState>,
    user: AuthUser,
    Path(letter_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let deleted = conn
        .execute(
            "DELETE FROM letters WHERE id = ?1 AND relation_id = ?2 AND sender_id = ?3",
            params![letter_id, relation.id, user.id],
        )
        .map_err(ApiError::from)?;
    if deleted == 0 {
        return Err(ApiError::not_found(
            "no such letter (only the sender can delete)",
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- pokes

async fn send_poke(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<SendPokeRequest>,
) -> Result<(StatusCode, Json<PokeRecord>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    // server-side cooldown per sender
    let last: Option<i64> = conn
        .query_row(
            "SELECT MAX(created_at) FROM pokes WHERE relation_id = ?1 AND sender_id = ?2",
            params![relation.id, user.id],
            |row| row.get(0),
        )
        .map_err(ApiError::from)?;
    if let Some(last) = last {
        let elapsed = now - last;
        if elapsed < limits::POKE_COOLDOWN_MS {
            return Err(ApiError::rate_limited(limits::POKE_COOLDOWN_MS - elapsed));
        }
    }
    let id = uuid::Uuid::now_v7().to_string();
    match conn.execute(
        "INSERT INTO pokes (id, relation_id, sender_id, client_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, relation.id, user.id, req.client_id, now],
    ) {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = conn
                .query_row(
                    "SELECT id, sender_id, created_at FROM pokes
                     WHERE relation_id = ?1 AND client_id = ?2",
                    params![relation.id, req.client_id],
                    |row| {
                        Ok(PokeRecord {
                            id: row.get(0)?,
                            sender_id: row.get(1)?,
                            created_at: row.get(2)?,
                        })
                    },
                )
                .map_err(ApiError::from)?;
            return Ok((StatusCode::OK, Json(existing)));
        }
        Err(e) => return Err(e.into()),
    }
    touch_relation(&conn, &relation.id).map_err(ApiError::from)?;
    let poke = PokeRecord {
        id: id.clone(),
        sender_id: user.id.clone(),
        created_at: now,
    };
    events::emit(
        &state,
        &conn,
        &relation.id,
        insync_core::EventKind::PokeReceived,
        &user.id,
        insync_core::EventData::Poke { poke_id: id },
    )?;
    Ok((StatusCode::CREATED, Json(poke)))
}

// ---------------------------------------------------------------- overview

fn unread_count(
    conn: &rusqlite::Connection,
    relation_id: &str,
    partner_id: &str,
    my_read_seq: i64,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM messages
         WHERE relation_id = ?1 AND sender_id = ?2 AND seq > ?3",
        params![relation_id, partner_id, my_read_seq],
        |row| row.get(0),
    )
}

fn my_read_seq(
    conn: &rusqlite::Connection,
    relation_id: &str,
    user_id: &str,
) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(MAX(last_message_seq), 0) FROM message_reads
         WHERE relation_id = ?1 AND user_id = ?2",
        params![relation_id, user_id],
        |row| row.get(0),
    )
}

async fn overview(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<OverviewResponse>, ApiError> {
    let conn = state.db.lock();
    let now = Db::now_ms();
    let me = public_user(&conn, &user.id).map_err(ApiError::from)?;
    let relation = relation_for(&conn, &user.id)?;
    let Some(relation) = relation.filter(|r| r.is_active()) else {
        return Ok(Json(OverviewResponse {
            overview: Overview {
                user: me,
                relation: None,
                partner: None,
                partner_presence: None,
                unread_messages: 0,
                last_message: None,
                my_read_seq: 0,
                unread_letters: 0,
                recent_pokes: 0,
                active_focus: None,
            },
        }));
    };
    let partner_id = relation
        .partner_of(&user.id)
        .ok_or_else(|| ApiError::forbidden("not a member"))?
        .to_string();
    let partner = public_user(&conn, &partner_id).map_err(ApiError::from)?;
    let read_seq = my_read_seq(&conn, &relation.id, &user.id).map_err(ApiError::from)?;
    let unread_messages =
        unread_count(&conn, &relation.id, &partner_id, read_seq).map_err(ApiError::from)?;
    let last_message = conn
        .query_row(
            "SELECT seq, id, sender_id, body, created_at FROM messages
             WHERE relation_id = ?1 ORDER BY seq DESC LIMIT 1",
            [&relation.id],
            row_message,
        )
        .ok();
    let unread_letters: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM letters
             WHERE relation_id = ?1 AND sender_id = ?2 AND read_at IS NULL",
            params![relation.id, partner_id],
            |row| row.get(0),
        )
        .map_err(ApiError::from)?;
    let recent_pokes: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pokes
             WHERE relation_id = ?1 AND sender_id = ?2 AND created_at > ?3",
            params![
                relation.id,
                partner_id,
                now - limits::RECENT_INTERACTION_WINDOW_MS
            ],
            |row| row.get(0),
        )
        .map_err(ApiError::from)?;
    let partner_presence = load_presence(&conn, &partner_id).map_err(ApiError::from)?;
    let active_focus = load_active_focus(&conn, &relation.id).map_err(ApiError::from)?;
    Ok(Json(OverviewResponse {
        overview: Overview {
            user: me,
            relation: Some(relation),
            partner: Some(partner),
            partner_presence,
            unread_messages: unread_messages.max(0) as u64,
            last_message,
            my_read_seq: read_seq as u64,
            unread_letters: unread_letters.max(0) as u64,
            recent_pokes: (recent_pokes.max(0) as u64).min(3),
            active_focus,
        },
    }))
}

pub(crate) fn load_presence(
    conn: &rusqlite::Connection,
    user_id: &str,
) -> Result<Option<Presence>, rusqlite::Error> {
    conn.query_row(
        "SELECT state, online, last_seen_at, focus_session_id FROM presence WHERE user_id = ?1",
        [user_id],
        |row| {
            let state: String = row.get(0)?;
            Ok(Presence {
                user_id: user_id.to_string(),
                state: serde_json::from_str(&state).unwrap_or(PresenceState::Online),
                online: row.get::<_, i64>(1)? != 0,
                last_seen_at: row.get(2)?,
                focus_session_id: row.get(3)?,
            })
        },
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

pub(crate) fn load_active_focus(
    conn: &rusqlite::Connection,
    relation_id: &str,
) -> Result<Option<insync_core::FocusSession>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, initiator_id, topic, duration_secs, state, revision, invited_at,
                accepted_at, started_at, end_at, paused_ms, paused_at, ended_at, ended_early
         FROM focus_sessions
         WHERE relation_id = ?1 AND state IN ('invited', 'active', 'paused')
         ORDER BY invited_at DESC LIMIT 1",
        [relation_id],
        row_focus,
    )
    .map(Some)
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(other),
    })
}

pub(crate) fn row_focus(
    row: &rusqlite::Row<'_>,
) -> Result<insync_core::FocusSession, rusqlite::Error> {
    let state: String = row.get(4)?;
    Ok(insync_core::FocusSession {
        id: row.get(0)?,
        initiator_id: row.get(1)?,
        topic: row.get(2)?,
        duration_secs: row.get(3)?,
        state: crate::routes::focus::state_from_db(&state),
        revision: row.get::<_, i64>(5)? as u64,
        invited_at: row.get(6)?,
        accepted_at: row.get(7)?,
        started_at: row.get(8)?,
        end_at: row.get(9)?,
        paused_ms: row.get(10)?,
        paused_at: row.get(11)?,
        ended_at: row.get(12)?,
        ended_early: row.get::<_, i64>(13)? != 0,
    })
}

// ---------------------------------------------------------------- events

async fn list_events(
    State(state): State<AppState>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<EventPage>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let after: u64 = q.get("after").and_then(|v| v.parse().ok()).unwrap_or(0);
    let limit: i64 = q
        .get("limit")
        .and_then(|v| v.parse().ok())
        .map(|l: i64| l.clamp(1, 500))
        .unwrap_or(200);
    let events_list =
        events::load_after(&conn, &relation.id, after, limit).map_err(ApiError::from)?;
    let latest = events::latest_seq(&conn, &relation.id).map_err(ApiError::from)?;
    Ok(Json(EventPage {
        events: events_list,
        latest_seq: latest,
    }))
}
