//! InSync backend: auth, pairing, chat, letters, pokes, focus sessions,
//! shared items and realtime event delivery. See docs/insync/protocol.md.

pub mod auth;
pub mod db;
pub mod error;
pub mod events;
pub mod maintenance;
pub mod routes;
pub mod state;
pub mod ws;

use axum::Router;
use axum::routing::get;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub db_path: PathBuf,
    pub bind_addr: SocketAddr,
    /// Bearer token lifetime.
    pub token_ttl: Duration,
    /// SQLite journal mode: WAL is the production default; tests may use
    /// in-memory databases where WAL is meaningless.
    pub wal: bool,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, String> {
        let port: u16 = std::env::var("INSYNC_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8787);
        let db_path = std::env::var("INSYNC_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("insync.db"));
        let ttl_days: u64 = std::env::var("INSYNC_TOKEN_TTL_DAYS")
            .ok()
            .and_then(|d| d.parse().ok())
            .unwrap_or(30);
        Ok(Self {
            db_path,
            bind_addr: SocketAddr::from(([0, 0, 0, 0], port)),
            token_ttl: Duration::from_secs(ttl_days * 24 * 60 * 60),
            wal: true,
        })
    }
}

/// Health probe used by deployment tooling.
async fn health() -> &'static str {
    "ok"
}

/// Version banner for the health endpoint and deploy checks.
async fn version() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "service": "insync-server",
        "protocol_version": insync_core::PROTOCOL_VERSION,
    }))
}

/// Builds the complete REST + WS router.
pub fn build_router(state: state::AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
        .nest("/v1/auth", routes::auth::router())
        .nest("/v1", routes::pair::router())
        .nest("/v1", routes::chat::router())
        .nest("/v1", routes::focus::router())
        .nest("/v1", routes::items::router())
        .nest("/v1", routes::presence::router())
        .route("/v1/ws", get(ws::upgrade))
        .fallback(|| async { error::ApiError::not_found("no such endpoint") })
        .layer(axum::extract::DefaultBodyLimit::max(64 * 1024))
        .with_state(state)
}

/// The server binary's only output: one line on stdout at startup. The
/// desktop client owns structured logging; the server stays log-free.
#[allow(clippy::print_stderr)]
fn startup_log(line: &str) {
    eprintln!("{line}");
}

/// Opens the database, runs migrations, serves forever. Also spawns the
/// maintenance task that finalizes focus sessions and prunes retention.
pub async fn serve(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let db = db::Db::open(&config.db_path, config.wal)?;
    let state = state::AppState::new(db, config.token_ttl)?;
    let maintenance_state = state.clone();
    tokio::spawn(async move {
        maintenance::run(maintenance_state).await;
    });
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    startup_log(&format!("insync-server listening on {}", config.bind_addr));
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .map_err(Into::into)
}
