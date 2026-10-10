//! Account endpoints: register, login, token refresh, logout.

use crate::auth::{self, AuthUser};
use crate::db::Db;
use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use insync_core::PublicUser;
use insync_core::api::{AuthRequest, AuthResponse, RegisterRequest, TokenResponse, error_codes};
use rusqlite::params;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
}

fn create_token_row(
    conn: &rusqlite::Connection,
    user_id: &str,
    ttl: std::time::Duration,
) -> Result<(String, i64), ApiError> {
    let token = auth::new_token();
    let now = Db::now_ms();
    let expires_at = now + ttl.as_millis() as i64;
    conn.execute(
        "INSERT INTO auth_tokens (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        params![auth::hash_token(&token), user_id, now, expires_at],
    )
    .map_err(ApiError::from)?;
    Ok((token, expires_at))
}

async fn register(
    State(state): State<AppState>,
    ConnectInfo(ip): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), ApiError> {
    let ip_key = format!("register:{}", ip_key(&ip));
    if let Err(retry) = state.rate.check(&ip_key, 10, 60 * 60 * 1000, Db::now_ms()) {
        return Err(ApiError::rate_limited(retry));
    }
    auth::validate_credentials(&req.username, &req.password)?;
    if req.display_name.trim().is_empty() || req.display_name.chars().count() > 64 {
        return Err(ApiError::validation(
            "display_name must be 1..=64 characters",
        ));
    }
    let conn = state.db.lock();
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM users WHERE username = ?1 COLLATE NOCASE",
            [&req.username],
            |_| Ok(()),
        )
        .is_ok();
    if exists {
        return Err(ApiError::conflict(
            error_codes::USERNAME_TAKEN,
            "username is taken",
        ));
    }
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = Db::now_ms();
    let hash = auth::hash_password(&req.password).map_err(ApiError::bad_request)?;
    conn.execute(
        "INSERT INTO users (id, username, display_name, password_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![user_id, req.username, req.display_name.trim(), hash, now],
    )
    .map_err(ApiError::from)?;
    let (token, expires_at) = create_token_row(&conn, &user_id, state.token_ttl)?;
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            user: PublicUser {
                id: user_id,
                username: req.username,
                display_name: req.display_name.trim().to_string(),
                created_at: now,
            },
            token,
            expires_at,
        }),
    ))
}

fn ip_key(ip: &std::net::SocketAddr) -> String {
    ip.ip().to_string()
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(ip): ConnectInfo<std::net::SocketAddr>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let key = format!("login-fail:{}:{}", ip_key(&ip), req.username.to_lowercase());
    let conn = state.db.lock();
    let row = conn
        .query_row(
            "SELECT id, password_hash FROM users WHERE username = ?1 COLLATE NOCASE",
            [&req.username],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok();
    let Some((user_id, hash)) = row else {
        drop(conn);
        if let Err(retry) = state.rate.check(&key, 10, 15 * 60 * 1000, Db::now_ms()) {
            return Err(ApiError::rate_limited(retry));
        }
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            error_codes::BAD_CREDENTIALS,
            "invalid username or password",
        ));
    };
    if !auth::verify_password(&req.password, &hash) {
        drop(conn);
        if let Err(retry) = state.rate.check(&key, 10, 15 * 60 * 1000, Db::now_ms()) {
            return Err(ApiError::rate_limited(retry));
        }
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            error_codes::BAD_CREDENTIALS,
            "invalid username or password",
        ));
    }
    state.rate.clear(&key);
    let (token, expires_at) = create_token_row(&conn, &user_id, state.token_ttl)?;
    let username: String = conn
        .query_row(
            "SELECT username FROM users WHERE id = ?1",
            [&user_id],
            |r| r.get(0),
        )
        .map_err(ApiError::from)?;
    let display_name: String = conn
        .query_row(
            "SELECT display_name FROM users WHERE id = ?1",
            [&user_id],
            |r| r.get(0),
        )
        .map_err(ApiError::from)?;
    let created_at: i64 = conn
        .query_row(
            "SELECT created_at FROM users WHERE id = ?1",
            [&user_id],
            |r| r.get(0),
        )
        .map_err(ApiError::from)?;
    Ok(Json(AuthResponse {
        user: PublicUser {
            id: user_id,
            username,
            display_name,
            created_at,
        },
        token,
        expires_at,
    }))
}

async fn refresh(
    State(state): State<AppState>,
    user: AuthUser,
    headers: axum::http::HeaderMap,
) -> Result<Json<TokenResponse>, ApiError> {
    // rotate: revoke the presented token, issue a fresh one
    let old_token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    let conn = state.db.lock();
    conn.execute(
        "UPDATE auth_tokens SET revoked_at = ?1 WHERE token_hash = ?2 AND revoked_at IS NULL",
        params![Db::now_ms(), auth::hash_token(&old_token)],
    )
    .map_err(ApiError::from)?;
    let (token, expires_at) = create_token_row(&conn, &user.id, state.token_ttl)?;
    Ok(Json(TokenResponse { token, expires_at }))
}

async fn logout(
    State(state): State<AppState>,
    user: AuthUser,
    headers: axum::http::HeaderMap,
) -> Result<StatusCode, ApiError> {
    // revoke only the presented token: other devices stay signed in
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default()
        .to_string();
    let conn = state.db.lock();
    conn.execute(
        "UPDATE auth_tokens SET revoked_at = ?1 WHERE token_hash = ?2 AND user_id = ?3",
        params![Db::now_ms(), auth::hash_token(&token), user.id],
    )
    .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}
