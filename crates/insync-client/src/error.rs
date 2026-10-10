//! Typed client failures the UI can branch on.

use insync_core::api::ErrorBody;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("token expired")]
    TokenExpired,
    #[error("relationship inactive")]
    RelationInactive,
    #[error("rate limited, retry in {retry_after_ms}ms")]
    RateLimited { retry_after_ms: i64 },
    #[error("conflict {code}: {message}")]
    Conflict { code: String, message: String },
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {0}")]
    Validation(String),
    #[error("network: {0}")]
    Network(String),
    #[error("server error {status}: {message}")]
    Server { status: u16, message: String },
}

impl ClientError {
    /// Coarse classification for outbox retry policy and UI messaging.
    pub fn kind(&self) -> FailureKind {
        match self {
            ClientError::Unauthorized(_)
            | ClientError::TokenExpired
            | ClientError::RelationInactive
            | ClientError::NotFound(_)
            | ClientError::Validation(_) => FailureKind::Permanent,
            ClientError::RateLimited { .. } | ClientError::Conflict { .. } => {
                FailureKind::Permanent
            }
            ClientError::Network(_) | ClientError::Server { .. } => FailureKind::Transient,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// Retrying cannot help; surface to the user.
    Permanent,
    /// Retrying after backoff makes sense.
    Transient,
}

/// Maps a `reqwest` response with an error status onto the typed error.
pub fn error_from_response(status: u16, body: &str) -> ClientError {
    if let Ok(parsed) = serde_json::from_str::<ErrorBody>(body) {
        use insync_core::api::error_codes;
        let message = parsed.error.message;
        return match parsed.error.code.as_str() {
            error_codes::UNAUTHORIZED => ClientError::Unauthorized(message),
            error_codes::TOKEN_EXPIRED => ClientError::TokenExpired,
            error_codes::RELATION_INACTIVE => ClientError::RelationInactive,
            error_codes::RATE_LIMITED => ClientError::RateLimited {
                retry_after_ms: 2000,
            },
            error_codes::NOT_FOUND => ClientError::NotFound(message),
            error_codes::VALIDATION => ClientError::Validation(message),
            code => ClientError::Conflict {
                code: code.to_string(),
                message,
            },
        };
    }
    if (500..600).contains(&status) {
        return ClientError::Server {
            status,
            message: body.chars().take(200).collect(),
        };
    }
    match status {
        401 => ClientError::Unauthorized(body.chars().take(80).collect()),
        403 => ClientError::RelationInactive,
        404 => ClientError::NotFound(body.chars().take(80).collect()),
        // 409 carries the note-conflict payload; the client re-fetches the
        // authoritative state through GET before surfacing it
        409 => ClientError::Conflict {
            code: insync_core::api::error_codes::CONFLICT.to_string(),
            message: body.chars().take(120).collect(),
        },
        429 => ClientError::RateLimited {
            retry_after_ms: 2000,
        },
        _ => ClientError::Server {
            status,
            message: format!(
                "unexpected response: {}",
                body.chars().take(80).collect::<String>()
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_bodies_map_to_typed_variants() {
        let body = r#"{"error":{"code":"TOKEN_EXPIRED","message":"token expired"}}"#;
        assert_eq!(error_from_response(401, body), ClientError::TokenExpired);
        let body = r#"{"error":{"code":"RELATION_INACTIVE","message":"dissolved"}}"#;
        assert_eq!(
            error_from_response(403, body),
            ClientError::RelationInactive
        );
        assert_eq!(
            error_from_response(500, "boom").kind(),
            FailureKind::Transient
        );
        assert_eq!(
            error_from_response(404, "no json").kind(),
            FailureKind::Permanent
        );
    }
}
