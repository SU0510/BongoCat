//! Periodic server maintenance: finalize due focus sessions, expire stale
//! invites, prune retention-limited events and poke rows.

use crate::db::Db;
use crate::error::ApiError;
use crate::events;
use crate::state::AppState;
use insync_core::{EventData, EventKind, FocusState};
use rusqlite::params;
use std::time::Duration;

pub const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(5);
/// How long ephemeral events stay queryable.
pub const EPHEMERAL_EVENT_RETENTION_MS: i64 = 24 * 60 * 60 * 1000;
/// Durable event-log tail kept per relation for catch-up.
pub const DURABLE_EVENT_KEEP: i64 = 500;
/// Pokes are interaction noise: bounded retention.
pub const POKE_RETENTION_MS: i64 = insync_core::limits::POKE_RETENTION_MS;

pub async fn run(state: AppState) {
    let mut interval = tokio::time::interval(MAINTENANCE_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        interval.tick().await;
        let _ = tick(&state);
    }
}

/// One maintenance pass; public so tests can drive it deterministically.
/// Returns (completed focus sessions, cancelled stale invites).
pub fn tick(state: &AppState) -> Result<(usize, usize), ApiError> {
    let conn = state.db.lock();
    let now = Db::now_ms();
    let completed = finalize_due_focus(state, &conn, now)?;
    let cancelled = cancel_stale_invites(&conn)?;
    prune_retention(&conn, now)?;
    Ok((completed, cancelled))
}

/// Sessions whose `end_at` passed while active become `completed` exactly
/// once (the state guard makes the event idempotent).
fn finalize_due_focus(
    state: &AppState,
    conn: &rusqlite::Connection,
    now: i64,
) -> Result<usize, ApiError> {
    let due: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, relation_id, initiator_id FROM focus_sessions
                 WHERE state = 'active' AND end_at IS NOT NULL AND end_at <= ?1",
            )
            .map_err(ApiError::from)?;
        let rows = stmt
            .query_map(params![now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(ApiError::from)?;
        rows.filter_map(Result::ok).collect()
    };
    let mut count = 0;
    for (id, relation_id, initiator_id) in due {
        let mut session = conn
            .query_row(
                "SELECT id, initiator_id, topic, duration_secs, state, revision, invited_at,
                        accepted_at, started_at, end_at, paused_ms, paused_at, ended_at, ended_early
                 FROM focus_sessions WHERE id = ?1",
                [&id],
                crate::routes::chat::row_focus,
            )
            .map_err(ApiError::from)?;
        if session.state != FocusState::Active || !session.due_for_completion(now) {
            continue;
        }
        session.state = FocusState::Completed;
        session.ended_at = Some(now);
        session.revision += 1;
        conn.execute(
            "UPDATE focus_sessions SET state = ?1, revision = ?2, ended_at = ?3 WHERE id = ?4",
            params![
                crate::routes::focus::state_to_db(&session.state),
                session.revision as i64,
                now,
                id
            ],
        )
        .map_err(ApiError::from)?;
        conn.execute(
            "UPDATE presence SET focus_session_id = NULL WHERE focus_session_id = ?1",
            [&id],
        )
        .map_err(ApiError::from)?;
        events::emit(
            state,
            conn,
            &relation_id,
            EventKind::FocusCompleted,
            &initiator_id,
            EventData::Focus(Box::new(session)),
        )?;
        count += 1;
    }
    Ok(count)
}

/// Invites nobody accepted for `FOCUS_INVITE_TIMEOUT_MS` become cancelled.
fn cancel_stale_invites(conn: &rusqlite::Connection) -> Result<usize, ApiError> {
    let now = Db::now_ms();
    let cancelled = conn
        .execute(
            "UPDATE focus_sessions SET state = 'cancelled', ended_at = ?1
             WHERE state = 'invited' AND invited_at <= ?2",
            params![now, now - insync_core::limits::FOCUS_INVITE_TIMEOUT_MS],
        )
        .map_err(ApiError::from)?;
    Ok(cancelled)
}

fn prune_retention(conn: &rusqlite::Connection, now: i64) -> Result<(), ApiError> {
    events::prune(conn, EPHEMERAL_EVENT_RETENTION_MS, DURABLE_EVENT_KEEP, now)
        .map_err(ApiError::from)?;
    conn.execute(
        "DELETE FROM pokes WHERE created_at < ?1",
        [now - POKE_RETENTION_MS],
    )
    .map_err(ApiError::from)?;
    Ok(())
}
