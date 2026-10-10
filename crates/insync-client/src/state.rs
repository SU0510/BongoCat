//! The engine's shared runtime state: a snapshot the UI can read on any
//! thread plus the events the engine pushes out.

use insync_core::{
    CoupleRelation, EventFrame, FocusSession, Letter, Message, Overview, Presence, PublicUser,
    SharedAnniversary, SharedNote, SharedTodo,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// Connection health as the UI should render it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncStatus {
    /// Signed out or not started.
    #[default]
    Offline,
    Connecting,
    Online,
    /// Retrying after a failure; the value is the attempt number.
    Retrying(u32),
    /// The token expired or was revoked; the user must sign in again.
    NeedsRelogin,
}

/// The runtime mirror of everything the UI renders.
#[derive(Debug, Default)]
pub struct EngineSnapshot {
    pub me: Option<PublicUser>,
    pub relation: Option<CoupleRelation>,
    pub partner: Option<PublicUser>,
    pub partner_presence: Option<Presence>,
    pub messages: Vec<Message>,
    pub letters: Vec<Letter>,
    pub shared_todos: Vec<SharedTodo>,
    pub shared_notes: Vec<SharedNote>,
    pub shared_anniversaries: Vec<SharedAnniversary>,
    pub active_focus: Option<FocusSession>,
    pub overview: Option<Overview>,
}

/// Everything the UI reads, guarded by one RwLock.
#[derive(Default)]
pub struct SharedState {
    snapshot: RwLock<EngineSnapshot>,
    status: Mutex<SyncStatus>,
    unread_badge: AtomicU64,
    /// Bumped on every state change so cheap pollers can skip work.
    revision: AtomicU64,
}

impl SharedState {
    pub fn status(&self) -> SyncStatus {
        *self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn set_status(&self, status: SyncStatus) {
        *self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = status;
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    pub fn unread_messages(&self) -> u64 {
        self.unread_badge.load(Ordering::Relaxed)
    }

    pub fn set_unread_messages(&self, count: u64) {
        self.unread_badge.store(count, Ordering::Relaxed);
        self.revision.fetch_add(1, Ordering::Relaxed);
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Relaxed)
    }

    pub fn read<R>(&self, reader: impl FnOnce(&EngineSnapshot) -> R) -> Option<R> {
        self.snapshot.read().ok().map(|snapshot| reader(&snapshot))
    }

    pub fn update<R>(&self, updater: impl FnOnce(&mut EngineSnapshot) -> R) -> Option<R> {
        let result = self
            .snapshot
            .write()
            .ok()
            .map(|mut snapshot| updater(&mut snapshot));
        self.revision.fetch_add(1, Ordering::Relaxed);
        result
    }
}

/// Events the engine delivers to the application.
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// A realtime event from the partner (already de-duplicated).
    Event(Arc<EventFrame>),
    /// The connection status changed.
    StatusChanged(SyncStatus),
    /// The engine stored a fresh overview (badges etc.).
    OverviewRefreshed,
    /// A note patch hit a revision conflict; nothing was overwritten.
    NoteConflict { note_id: String },
    /// A poke could not be delivered (rate limit or relation problem).
    PokeFailed,
    /// An outbox op failed permanently.
    OpFailed { op: String, error: String },
}

pub type EventSender = std::sync::mpsc::SyncSender<EngineEvent>;
