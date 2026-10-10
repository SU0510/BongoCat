//! Typed API errors with the uniform `{"error": {code, message}}` body.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use insync_core::api::error_codes;
use insync_core::api::{ErrorBody, RateLimited};

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
    /// Set for RATE_LIMITED responses.
    pub retry_after_ms: Option<i64>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.to_string(),
            message: message.into(),
            retry_after_ms: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, error_codes::UNAUTHORIZED, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, error_codes::FORBIDDEN, message)
    }

    pub fn relation_inactive() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            error_codes::RELATION_INACTIVE,
            "this data belongs to a dissolved relationship",
        )
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, error_codes::NOT_FOUND, message)
    }

    pub fn conflict(code: &str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            error_codes::VALIDATION,
            message,
        )
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, error_codes::VALIDATION, message)
    }

    pub fn rate_limited(retry_after_ms: i64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: error_codes::RATE_LIMITED.to_string(),
            message: "slow down".into(),
            retry_after_ms: Some(retry_after_ms),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (
            self.status,
            Json(ErrorBody::new(&self.code, self.message.clone())),
        )
            .into_response();
        if let Some(retry) = self.retry_after_ms {
            response = (
                self.status,
                [("retry-after", retry.to_string())],
                Json(serde_json::json!({
                    "error": {"code": self.code, "message": self.message},
                    "retry_after_ms": retry,
                    "rate_limited": RateLimited { retry_after_ms: retry },
                })),
            )
                .into_response();
        }
        response
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::SqliteFailure(err, msg)
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                let text = msg.as_deref().unwrap_or("constraint violation");
                if text.contains("UNIQUE") {
                    Self::conflict(error_codes::CONFLICT, "already exists")
                } else {
                    Self::validation(format!("constraint violation: {text}"))
                }
            }
            _ => Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
                "storage error",
            ),
        }
    }
}

/// Unique-index hit for `(relation_id, client_id)` idempotency: treated as
/// success by callers (the object already exists).
pub fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(err, msg)
            if err.code == rusqlite::ErrorCode::ConstraintViolation
                && msg.as_deref().is_some_and(|m| m.contains("UNIQUE"))
    )
}
