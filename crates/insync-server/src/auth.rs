//! Authentication: Argon2id password hashing and hashed bearer tokens.
//!
//! Passwords use the raw `hash_password_into` API with a random 16-byte
//! salt and a self-describing storage string
//! `argon2id$<salt_hex>$<hash_hex>` (Argon2::default() parameters: v19,
//! m=19456, t=2, p=1), compared in constant time. This keeps the hash
//! format stable without depending on the young PHC helper APIs.

use crate::db::Db;
use crate::error::ApiError;
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use insync_core::api::error_codes;
use rand::TryRng;
use rand::rngs::SysRng;
use sha2::{Digest, Sha256};
use std::time::Duration;

const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;

/// Random bytes from the OS CSPRNG.
fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    SysRng
        .try_fill_bytes(&mut buf)
        .expect("OS entropy unavailable");
    buf
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = random_bytes(SALT_LEN);
    let mut out = vec![0u8; HASH_LEN];
    Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "argon2id${}${}",
        hex_encode(&salt),
        hex_encode(&out)
    ))
}

pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 3 || parts[0] != "argon2id" {
        return false;
    }
    let (Some(salt), Some(expected)) = (hex_decode(parts[1]), hex_decode(parts[2])) else {
        return false;
    };
    if salt.len() != SALT_LEN || expected.len() != HASH_LEN {
        return false;
    }
    let mut out = vec![0u8; HASH_LEN];
    if Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut out)
        .is_err()
    {
        return false;
    }
    constant_time_eq(&out, &expected)
}

/// A fresh 256-bit bearer token (hex-encoded for transport friendliness).
pub fn new_token() -> String {
    hex_encode(&random_bytes(32))
}

pub fn hash_token(token: &str) -> String {
    hex_encode(&Sha256::digest(token.as_bytes()))
}

const USERNAME_MIN: usize = 3;
const USERNAME_MAX: usize = 32;
const PASSWORD_MIN: usize = 8;
const PASSWORD_MAX: usize = 128;

pub fn validate_credentials(username: &str, password: &str) -> Result<(), ApiError> {
    let ok_username = (USERNAME_MIN..=USERNAME_MAX).contains(&username.len())
        && username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok_username {
        return Err(ApiError::validation(
            "username must be 3-32 characters of [a-z0-9_-]",
        ));
    }
    if !(PASSWORD_MIN..=PASSWORD_MAX).contains(&password.len()) {
        return Err(ApiError::validation(format!(
            "password must be {PASSWORD_MIN}..={PASSWORD_MAX} characters"
        )));
    }
    Ok(())
}

/// The authenticated user, resolved from `Authorization: Bearer <token>`.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

pub const TOKEN_TTL_FALLBACK: Duration = Duration::from_secs(30 * 24 * 3600);

impl FromRequestParts<crate::state::AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::state::AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;
        authenticate_token(state, header.trim())
    }
}

/// Validates a token against the store: exists, unrevoked, unexpired.
pub fn authenticate_token(
    state: &crate::state::AppState,
    token: &str,
) -> Result<AuthUser, ApiError> {
    let token_hash = hash_token(token);
    let now = Db::now_ms();
    let conn = state.db.lock();
    let row = conn
        .query_row(
            "SELECT u.id, u.username, u.display_name, t.expires_at, t.revoked_at
             FROM auth_tokens t JOIN users u ON u.id = t.user_id
             WHERE t.token_hash = ?1",
            [&token_hash],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                ))
            },
        )
        .map_err(|_| ApiError::unauthorized("invalid or expired token"))?;
    let (id, username, display_name, expires_at, revoked_at) = row;
    if revoked_at.is_some() {
        return Err(ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            error_codes::TOKEN_EXPIRED,
            "token was revoked",
        ));
    }
    if expires_at <= now {
        return Err(ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            error_codes::TOKEN_EXPIRED,
            "token expired",
        ));
    }
    Ok(AuthUser {
        id,
        username,
        display_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hashes_are_salt_and_verify() {
        let h1 = hash_password("correct horse").unwrap();
        let h2 = hash_password("correct horse").unwrap();
        assert_ne!(h1, h2, "salts must differ");
        assert!(h1.starts_with("argon2id$"));
        assert!(verify_password("correct horse", &h1));
        assert!(!verify_password("wrong horse", &h1));
        assert!(!verify_password("correct horse", "argon2id$zz$zz"));
    }

    #[test]
    fn tokens_are_random_and_hashable() {
        let t1 = new_token();
        let t2 = new_token();
        assert_ne!(t1, t2);
        assert_eq!(t1.len(), 64);
        assert_eq!(hash_token(&t1), hash_token(&t1));
        assert_ne!(hash_token(&t1), hash_token(&t2));
    }

    #[test]
    fn hex_round_trip() {
        let bytes = random_bytes(31);
        let decoded = hex_decode(&hex_encode(&bytes)).unwrap();
        assert_eq!(decoded, bytes);
        assert!(hex_decode("zz").is_none());
        assert!(hex_decode("abc").is_none());
    }

    #[test]
    fn credential_shape_validation() {
        assert!(validate_credentials("ab", "longenough1").is_err());
        assert!(validate_credentials("ok_name", "short").is_err());
        assert!(validate_credentials("bad name!", "longenough1").is_err());
        assert!(validate_credentials("ok_name", "longenough1").is_ok());
    }
}
