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

/// One-shot blocking execution for infrequent REST calls made outside the
/// sync engine (account operations from the UI). Each call spins a
/// throwaway current-thread runtime; never use this on the engine thread.
pub mod blocking {
    pub fn run<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("blocking runtime")
            .block_on(future)
    }
}
