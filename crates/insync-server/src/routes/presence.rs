//! Presence: read the partner's availability, set own status. Connection
//! online/offline transitions live in the WebSocket handler.

use crate::auth::AuthUser;
use crate::db::Db;
use crate::error::ApiError;
use crate::events;
use crate::routes::{public_user, relation_for};
use crate::state::AppState;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use insync_core::api::PresenceRequest;
use insync_core::{EventData, EventKind, Presence, PresenceState};
use rusqlite::params;

pub fn router() -> Router<AppState> {
    Router::new().route("/presence", get(get_presence).post(set_presence))
}

async fn get_presence(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Option<Presence>>, ApiError> {
    let conn = state.db.lock();
    let Some(relation) = relation_for(&conn, &user.id)?.filter(|r| r.is_active()) else {
        return Ok(Json(None));
    };
    let partner_id = relation
        .partner_of(&user.id)
        .ok_or_else(|| ApiError::forbidden("not a member"))?;
    let presence = crate::routes::chat::load_presence(&conn, partner_id).map_err(ApiError::from)?;
    Ok(Json(presence))
}

async fn set_presence(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<PresenceRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let conn = state.db.lock();
    let now = Db::now_ms();
    upsert_presence(&conn, &user.id, req.state, true, now)?;
    let Some(relation) = relation_for(&conn, &user.id)?.filter(|r| r.is_active()) else {
        return Ok(axum::http::StatusCode::NO_CONTENT);
    };
    let presence = crate::routes::chat::load_presence(&conn, &user.id)
        .map_err(ApiError::from)?
        .unwrap_or(Presence {
            user_id: user.id.clone(),
            state: req.state,
            online: true,
            last_seen_at: now,
            focus_session_id: None,
        });
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::PresenceUpdated,
        &user.id,
        EventData::Presence(Box::new(presence)),
    )?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Upsert used by both the REST route and the WebSocket lifecycle.
pub fn upsert_presence(
    conn: &rusqlite::Connection,
    user_id: &str,
    state: PresenceState,
    online: bool,
    now: i64,
) -> Result<(), ApiError> {
    conn.execute(
        "INSERT INTO presence (user_id, online, state, last_seen_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (user_id)
         DO UPDATE SET online = excluded.online, state = excluded.state,
                       last_seen_at = excluded.last_seen_at",
        params![
            user_id,
            online as i64,
            serde_json::to_string(&state).unwrap_or_default(),
            now
        ],
    )
    .map_err(ApiError::from)?;
    Ok(())
}

/// Marks a user offline and emits presence to the partner (WS disconnect).
pub fn mark_offline(state: &AppState, user_id: &str) {
    let conn = state.db.lock();
    let now = Db::now_ms();
    if upsert_presence(&conn, user_id, PresenceState::Online, false, now).is_err() {
        return;
    }
    if let Ok(Some(relation)) = relation_for(&conn, user_id)
        && relation.is_active()
        && let Some(presence) = crate::routes::chat::load_presence(&conn, user_id)
            .ok()
            .flatten()
    {
        let _ = events::emit(
            state,
            &conn,
            &relation.id,
            EventKind::PresenceUpdated,
            user_id,
            EventData::Presence(Box::new(presence)),
        );
    }
}

/// Refreshes the heartbeat timestamp for a connected socket.
pub fn touch_online(state: &AppState, user_id: &str) {
    let conn = state.db.lock();
    let _ = conn.execute(
        "UPDATE presence SET online = 1, last_seen_at = ?1 WHERE user_id = ?2",
        params![Db::now_ms(), user_id],
    );
}

/// Partner display helper kept here to avoid circular imports in tests.
pub fn partner_name(state: &AppState, user_id: &str) -> Option<String> {
    let conn = state.db.lock();
    public_user(&conn, user_id).ok().map(|u| u.display_name)
}
