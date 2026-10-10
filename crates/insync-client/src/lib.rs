//! InSync's client side: the typed REST client, the WebSocket realtime
//! loop and the sync engine (outbox, reconnect, resync, de-duplication).
//! The engine runs on one dedicated OS thread with its own tokio runtime;
//! UI threads never block on the network.

pub mod error;
pub mod http;
pub mod state;
pub mod sync;

pub use error::{ClientError, FailureKind};
pub use http::InSyncClient;
pub use state::{EngineEvent, SharedState, SyncStatus};
pub use sync::{EngineConfig, EngineHandle, SyncCommand, TokenSource};
