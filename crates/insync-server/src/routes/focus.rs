//! Focus session lifecycle. The server applies every transition through the
//! shared `insync-core` table and broadcasts the resulting session, so both
//! clients converge on identical state.

use crate::auth::AuthUser;
use crate::db::Db;
use crate::error::ApiError;
use crate::events;
use crate::routes::require_active_relation;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use insync_core::api::{FocusInviteRequest, FocusResponse, error_codes};
use insync_core::{EventData, EventKind, FocusAction, FocusSession, FocusState};
use rusqlite::params;

/// States are stored as plain tokens (`invited`, `active`, ...) so SQL
/// literals can filter them directly.
pub fn state_to_db(state: &FocusState) -> String {
    serde_json::to_string(state)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

pub fn state_from_db(raw: &str) -> FocusState {
    serde_json::from_str(&format!("\"{raw}\"")).unwrap_or(FocusState::Invited)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/focus/invite", post(invite))
        .route("/focus/{id}/accept", post(accept))
        .route("/focus/{id}/decline", post(decline))
        .route("/focus/{id}/cancel", post(cancel))
        .route("/focus/{id}/pause", post(pause))
        .route("/focus/{id}/resume", post(resume))
        .route("/focus/{id}/finish-early", post(finish_early))
}

async fn invite(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<FocusInviteRequest>,
) -> Result<(StatusCode, Json<FocusResponse>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    // one live session per relation
    if let Some(existing) =
        crate::routes::chat::load_active_focus(&conn, &relation.id).map_err(ApiError::from)?
    {
        return Err(ApiError::conflict(
            error_codes::CONFLICT,
            format!("session already in state {:?}", existing.state),
        ));
    }
    let session = FocusSession::new(
        uuid::Uuid::now_v7().to_string(),
        user.id.clone(),
        req.topic.clone(),
        req.duration_secs,
        now,
    )
    .map_err(|e| ApiError::validation(e.to_string()))?;
    insert_session(&conn, &relation.id, req.client_id.clone(), &session)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::FocusInvited,
        &user.id,
        EventData::Focus(Box::new(session.clone())),
    )?;
    Ok((StatusCode::CREATED, Json(FocusResponse { focus: session })))
}

fn insert_session(
    conn: &rusqlite::Connection,
    relation_id: &str,
    client_id: String,
    session: &FocusSession,
) -> Result<(), ApiError> {
    conn.execute(
        "INSERT INTO focus_sessions (id, relation_id, initiator_id, client_id, topic,
            duration_secs, state, revision, invited_at, accepted_at, started_at, end_at,
            paused_ms, paused_at, ended_at, ended_early)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            session.id,
            relation_id,
            session.initiator_id,
            client_id,
            session.topic,
            session.duration_secs,
            state_to_db(&session.state),
            session.revision as i64,
            session.invited_at,
            session.accepted_at,
            session.started_at,
            session.end_at,
            session.paused_ms,
            session.paused_at,
            session.ended_at,
            session.ended_early as i64,
        ],
    )
    .map_err(ApiError::from)?;
    Ok(())
}

fn update_session(conn: &rusqlite::Connection, session: &FocusSession) -> Result<(), ApiError> {
    conn.execute(
        "UPDATE focus_sessions SET state = ?1, revision = ?2, accepted_at = ?3,
            started_at = ?4, end_at = ?5, paused_ms = ?6, paused_at = ?7,
            ended_at = ?8, ended_early = ?9
         WHERE id = ?10",
        params![
            state_to_db(&session.state),
            session.revision as i64,
            session.accepted_at,
            session.started_at,
            session.end_at,
            session.paused_ms,
            session.paused_at,
            session.ended_at,
            session.ended_early as i64,
            session.id,
        ],
    )
    .map_err(ApiError::from)?;
    Ok(())
}

/// Shared transition handler for all action endpoints.
async fn transition(
    state: &AppState,
    user: &AuthUser,
    focus_id: &str,
    action: FocusAction,
) -> Result<FocusSession, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let session = conn
        .query_row(
            "SELECT id, initiator_id, topic, duration_secs, state, revision, invited_at,
                    accepted_at, started_at, end_at, paused_ms, paused_at, ended_at, ended_early
             FROM focus_sessions WHERE id = ?1 AND relation_id = ?2",
            params![focus_id, relation.id],
            crate::routes::chat::row_focus,
        )
        .map_err(|_| ApiError::not_found("no such focus session"))?;
    if session.state.is_terminal() {
        return Err(ApiError::conflict(
            error_codes::CONFLICT,
            "session already finished",
        ));
    }
    let actor_is_initiator = session.initiator_id == user.id;
    let next = session
        .transition(action, actor_is_initiator, Db::now_ms())
        .map_err(|e| ApiError::conflict(error_codes::CONFLICT, e.to_string()))?;
    update_session(&conn, &next)?;

    // accept is its own notification; the resulting state change is the
    // precise event every client applies
    if action == FocusAction::Accept {
        events::emit(
            state,
            &conn,
            &relation.id,
            EventKind::FocusAccepted,
            &user.id,
            EventData::Focus(Box::new(next.clone())),
        )?;
    }
    let kind = match next.state {
        FocusState::Active => EventKind::FocusStarted,
        FocusState::Paused => EventKind::FocusPaused,
        FocusState::Declined => EventKind::FocusDeclined,
        FocusState::Cancelled => EventKind::FocusCancelled,
        FocusState::Completed => EventKind::FocusCompleted,
        FocusState::Invited => EventKind::FocusInvited,
    };
    events::emit(
        state,
        &conn,
        &relation.id,
        kind,
        &user.id,
        EventData::Focus(Box::new(next.clone())),
    )?;
    // presence carries the active focus so the partner sees "focusing"
    if next.state == FocusState::Active {
        conn.execute(
            "UPDATE presence SET focus_session_id = ?1 WHERE user_id = ?2",
            params![next.id, user.id],
        )
        .map_err(ApiError::from)?;
    } else {
        conn.execute(
            "UPDATE presence SET focus_session_id = NULL WHERE user_id = ?1",
            params![user.id],
        )
        .map_err(ApiError::from)?;
    }
    Ok(next)
}

macro_rules! action_endpoint {
    ($name:ident, $action:expr) => {
        async fn $name(
            State(state): State<AppState>,
            user: AuthUser,
            Path(focus_id): Path<String>,
        ) -> Result<Json<FocusResponse>, ApiError> {
            let next = transition(&state, &user, &focus_id, $action).await?;
            Ok(Json(FocusResponse { focus: next }))
        }
    };
}

action_endpoint!(accept, FocusAction::Accept);
action_endpoint!(decline, FocusAction::Decline);
action_endpoint!(cancel, FocusAction::Cancel);
action_endpoint!(pause, FocusAction::Pause);
action_endpoint!(resume, FocusAction::Resume);
action_endpoint!(finish_early, FocusAction::FinishEarly);
