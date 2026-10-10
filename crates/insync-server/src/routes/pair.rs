//! Pairing: invite codes, accept, dissolve, and the /me aggregate.

use crate::auth::AuthUser;
use crate::db::Db;
use crate::error::ApiError;
use crate::events;
use crate::routes::{public_user, relation_for};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use insync_core::api::{
    AcceptInviteRequest, InviteResponse, MeResponse, PairResponse, error_codes,
};
use insync_core::{CoupleRelation, EventData, EventKind, RelationStatus};
use rand::RngExt;
use rusqlite::params;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/pair/invite", post_invite_route())
        .route("/pair/accept", post_accept_route())
        .route("/pair/dissolve", post_dissolve_route())
}

fn post_invite_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::post(invite)
}

fn post_accept_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::post(accept)
}

fn post_dissolve_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::post(dissolve)
}

/// Unambiguous invite-code alphabet (no 0/O/1/I).
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

fn new_invite_code() -> String {
    let mut rng = rand::rng();
    (0..8)
        .map(|_| CODE_ALPHABET[rng.random_range(0..CODE_ALPHABET.len())] as char)
        .collect()
}

async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<MeResponse>, ApiError> {
    let conn = state.db.lock();
    let relation = relation_for(&conn, &user.id)?;
    let partner = relation
        .as_ref()
        .and_then(|r| r.partner_of(&user.id))
        .map(|pid| public_user(&conn, pid))
        .transpose()
        .map_err(ApiError::from)?;
    Ok(Json(MeResponse {
        user: public_user(&conn, &user.id).map_err(ApiError::from)?,
        relation,
        partner,
        presence: None,
    }))
}

const INVITE_TTL_MS: i64 = 24 * 60 * 60 * 1000;
const INVITE_DAILY_LIMIT: u32 = 5;

async fn invite(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<(StatusCode, Json<InviteResponse>), ApiError> {
    let conn = state.db.lock();
    if relation_for(&conn, &user.id)?.is_some_and(|r| r.is_active()) {
        return Err(ApiError::conflict(
            error_codes::ALREADY_PAIRED,
            "already in a relationship",
        ));
    }
    let now = Db::now_ms();
    let day_bucket = format!("invite:{}:{}", user.id, now / (24 * 60 * 60 * 1000));
    if let Err(retry) = state
        .rate
        .check(&day_bucket, INVITE_DAILY_LIMIT, 24 * 60 * 60 * 1000, now)
    {
        return Err(ApiError::rate_limited(retry));
    }
    let code = new_invite_code();
    conn.execute(
        "INSERT INTO invites (code, created_by, expires_at, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![code, user.id, now + INVITE_TTL_MS, now],
    )
    .map_err(ApiError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(InviteResponse {
            code,
            expires_at: now + INVITE_TTL_MS,
        }),
    ))
}

async fn accept(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<AcceptInviteRequest>,
) -> Result<Json<PairResponse>, ApiError> {
    let conn = state.db.lock();
    if relation_for(&conn, &user.id)?.is_some_and(|r| r.is_active()) {
        return Err(ApiError::conflict(
            error_codes::ALREADY_PAIRED,
            "already in a relationship",
        ));
    }
    let now = Db::now_ms();
    let creator: String = conn
        .query_row(
            "SELECT created_by FROM invites WHERE code = ?1 AND used_at IS NULL AND expires_at > ?2",
            params![req.code.trim().to_uppercase(), now],
            |row| row.get(0),
        )
        .map_err(|_| ApiError::conflict(error_codes::INVITE_INVALID, "invite code is invalid or expired"))?;
    if creator == user.id {
        return Err(ApiError::conflict(
            error_codes::INVITE_INVALID,
            "cannot accept your own invite",
        ));
    }
    let relation_id = uuid::Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO relations (id, user_a, user_b, status, created_at, last_interaction_at)
         VALUES (?1, ?2, ?3, 'active', ?4, ?4)",
        params![relation_id, creator, user.id, now],
    )
    .map_err(ApiError::from)?;
    conn.execute(
        "UPDATE invites SET used_at = ?1, used_by = ?2 WHERE code = ?3",
        params![now, user.id, req.code.trim().to_uppercase()],
    )
    .map_err(ApiError::from)?;

    let relation = CoupleRelation {
        id: relation_id.clone(),
        user_a: creator.clone(),
        user_b: user.id.clone(),
        status: RelationStatus::Active,
        created_at: now,
        last_interaction_at: now,
        dissolved_at: None,
    };
    events::emit(
        &state,
        &conn,
        &relation_id,
        EventKind::InviteAccepted,
        &user.id,
        EventData::PartnerId {
            partner_id: user.id.clone(),
        },
    )?;
    let partner = public_user(&conn, &creator).map_err(ApiError::from)?;
    Ok(Json(PairResponse { relation, partner }))
}

async fn dissolve(State(state): State<AppState>, user: AuthUser) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let relation = relation_for(&conn, &user.id)?
        .ok_or_else(|| ApiError::forbidden("no partner relationship"))?;
    let now = Db::now_ms();
    conn.execute(
        "UPDATE relations SET status = 'dissolved', dissolved_at = ?1 WHERE id = ?2",
        params![now, relation.id],
    )
    .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::RelationDissolved,
        &user.id,
        EventData::RelationId {
            relation_id: relation.id.clone(),
        },
    )?;
    Ok(StatusCode::NO_CONTENT)
}
