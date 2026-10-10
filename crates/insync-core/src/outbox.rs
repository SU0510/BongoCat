//! The client outbox: local-first operations waiting for the network.
//!
//! Every queued op carries a stable idempotency key, so retrying after a
//! timeout can never create duplicates server-side. Short-lived interactions
//! (pokes) expire instead of replaying stale; everything else retries until
//! it succeeds or the relation dissolves.

use crate::{FocusAction, UnixMs, limits};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SyncOp {
    SendMessage {
        client_id: String,
        body: String,
    },
    MarkRead {
        last_message_seq: u64,
    },
    SendPoke {
        client_id: String,
    },
    SendLetter {
        client_id: String,
        body: String,
    },
    MarkLetterRead {
        letter_id: String,
    },
    FocusAction {
        focus_id: String,
        action: FocusAction,
        /// Server-side revision the local decision was based on; used for
        /// stale-action detection on reconnect.
        base_revision: u64,
    },
    FocusInvite {
        client_id: String,
        topic: Option<String>,
        duration_secs: i64,
    },
    CreateTodo {
        client_id: String,
        title: String,
        detail: Option<String>,
        priority: crate::Priority,
        due_at: Option<UnixMs>,
    },
    UpdateTodo {
        todo_id: String,
        req: crate::UpdateTodoRequest,
    },
    DeleteTodo {
        todo_id: String,
    },
    CreateNote {
        client_id: String,
        content: String,
    },
    UpdateNote {
        note_id: String,
        content: String,
        base_revision: u64,
    },
    DeleteNote {
        note_id: String,
    },
    CreateAnniversary {
        client_id: String,
        req: crate::CreateAnniversaryRequest,
    },
    UpdateAnniversary {
        anniversary_id: String,
        req: crate::UpdateAnniversaryRequest,
    },
    DeleteAnniversary {
        anniversary_id: String,
    },
    SetPresence {
        state: crate::PresenceState,
    },
}

impl SyncOp {
    /// The idempotency key that makes retries safe for create-style ops;
    /// `None` for pure state syncs that are safe to repeat.
    pub fn idempotency_key(&self) -> Option<&str> {
        match self {
            SyncOp::SendMessage { client_id, .. }
            | SyncOp::SendPoke { client_id }
            | SyncOp::SendLetter { client_id, .. }
            | SyncOp::FocusInvite { client_id, .. }
            | SyncOp::CreateTodo { client_id, .. }
            | SyncOp::CreateNote { client_id, .. }
            | SyncOp::CreateAnniversary { client_id, .. } => Some(client_id),
            SyncOp::MarkRead { .. }
            | SyncOp::MarkLetterRead { .. }
            | SyncOp::FocusAction { .. }
            | SyncOp::UpdateTodo { .. }
            | SyncOp::DeleteTodo { .. }
            | SyncOp::UpdateNote { .. }
            | SyncOp::DeleteNote { .. }
            | SyncOp::UpdateAnniversary { .. }
            | SyncOp::DeleteAnniversary { .. }
            | SyncOp::SetPresence { .. } => None,
        }
    }

    /// True when this op must not be sent while the relation is dissolved.
    pub fn requires_relation(&self) -> bool {
        !matches!(self, SyncOp::SetPresence { .. } | SyncOp::MarkRead { .. })
    }

    pub fn describe(&self) -> &'static str {
        match self {
            SyncOp::SendMessage { .. } => "send_message",
            SyncOp::MarkRead { .. } => "mark_read",
            SyncOp::SendPoke { .. } => "send_poke",
            SyncOp::SendLetter { .. } => "send_letter",
            SyncOp::MarkLetterRead { .. } => "mark_letter_read",
            SyncOp::FocusAction { .. } => "focus_action",
            SyncOp::FocusInvite { .. } => "focus_invite",
            SyncOp::CreateTodo { .. } => "create_todo",
            SyncOp::UpdateTodo { .. } => "update_todo",
            SyncOp::DeleteTodo { .. } => "delete_todo",
            SyncOp::CreateNote { .. } => "create_note",
            SyncOp::UpdateNote { .. } => "update_note",
            SyncOp::DeleteNote { .. } => "delete_note",
            SyncOp::CreateAnniversary { .. } => "create_anniversary",
            SyncOp::UpdateAnniversary { .. } => "update_anniversary",
            SyncOp::DeleteAnniversary { .. } => "delete_anniversary",
            SyncOp::SetPresence { .. } => "set_presence",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpState {
    Pending,
    InFlight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboxEntry {
    pub op: SyncOp,
    pub state: OpState,
    pub queued_at: UnixMs,
    pub attempts: u32,
    pub next_attempt_at: UnixMs,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueOutcome {
    Queued,
    /// Duplicate idempotency key with an identical op: ignored.
    Duplicate,
    /// Rejected locally (cooldown, expired, invalid).
    Rejected(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompleteOutcome {
    Done,
    Retry { after_ms: i64 },
    Dropped(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainedOp {
    pub entry: OutboxEntry,
}

/// In-memory outbox with a bounded queue, per-op retry backoff and TTL for
/// ephemeral ops. Serialization is up to the caller (persist via JSON).
#[derive(Debug, Default)]
pub struct Outbox {
    queue: VecDeque<OutboxEntry>,
    capacity: usize,
}

/// Base retry delay; doubles per attempt, capped.
pub const RETRY_BASE_MS: i64 = 2000;
pub const RETRY_MAX_MS: i64 = 5 * 60 * 1000;
/// After this many attempts a non-idempotent-safe op stops retrying.
pub const MAX_ATTEMPTS: u32 = 30;

impl Outbox {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = &OutboxEntry> {
        self.queue.iter()
    }

    /// Enqueue an op. Pokes within the server cooldown are rejected locally;
    /// ops with a duplicate idempotency key are ignored.
    pub fn enqueue(
        &mut self,
        op: SyncOp,
        now: UnixMs,
        last_poke_sent_at: Option<UnixMs>,
    ) -> EnqueueOutcome {
        if let SyncOp::SendPoke { .. } = op {
            if last_poke_sent_at.is_some_and(|t| now - t < limits::POKE_COOLDOWN_MS) {
                return EnqueueOutcome::Rejected("poke_cooldown");
            }
            if self
                .queue
                .iter()
                .any(|e| matches!(e.op, SyncOp::SendPoke { .. }))
            {
                return EnqueueOutcome::Rejected("poke_pending");
            }
        }
        if let Some(key) = op.idempotency_key()
            && self
                .queue
                .iter()
                .any(|e| e.op.idempotency_key() == Some(key) && e.op.describe() == op.describe())
        {
            return EnqueueOutcome::Duplicate;
        }
        if self.queue.len() >= self.capacity {
            return EnqueueOutcome::Rejected("outbox_full");
        }
        self.queue.push_back(OutboxEntry {
            op,
            state: OpState::Pending,
            queued_at: now,
            attempts: 0,
            next_attempt_at: now,
            last_error: None,
        });
        EnqueueOutcome::Queued
    }

    /// Take the next op ready to send. The entry stays in the queue marked
    /// [`OpState::InFlight`]; the caller must eventually call
    /// [`Outbox::complete`] with the drained op or [`Outbox::reset_in_flight`].
    pub fn take_ready(&mut self, now: UnixMs) -> Option<DrainedOp> {
        loop {
            let idx = self
                .queue
                .iter()
                .position(|e| e.state == OpState::Pending && e.next_attempt_at <= now)?;
            let entry = &self.queue[idx];
            if matches!(entry.op, SyncOp::SendPoke { .. })
                && now - entry.queued_at > limits::POKE_OUTBOX_TTL_MS
            {
                // drop expired ephemeral ops instead of sending stale pokes
                self.queue.remove(idx);
                continue;
            }
            let entry = &mut self.queue[idx];
            entry.state = OpState::InFlight;
            entry.attempts += 1;
            return Some(DrainedOp {
                entry: entry.clone(),
            });
        }
    }

    /// Report the outcome of an in-flight op.
    pub fn complete(&mut self, op: &SyncOp, outcome: CompleteOutcome, now: UnixMs) {
        let idx = self.queue.iter().position(|e| {
            e.state == OpState::InFlight
                && e.op.describe() == op.describe()
                && e.op.idempotency_key() == op.idempotency_key()
        });
        let Some(idx) = idx else { return };
        match outcome {
            CompleteOutcome::Done => {
                self.queue.remove(idx);
            }
            CompleteOutcome::Dropped(reason) => {
                let _ = reason;
                self.queue.remove(idx);
            }
            CompleteOutcome::Retry { after_ms } => {
                let mut entry = self.queue.remove(idx).expect("position verified");
                entry.state = OpState::Pending;
                entry.next_attempt_at = now + after_ms.max(RETRY_BASE_MS);
                // safety valve for ops that cannot converge
                if entry.attempts >= MAX_ATTEMPTS {
                    return;
                }
                self.queue.push_back(entry);
            }
        }
    }

    /// Backoff for the n-th retry (1-based): 2s, 4s, 8s ... capped at 5 min.
    pub fn backoff_ms(attempt: u32) -> i64 {
        let shift = attempt.min(9).saturating_sub(1);
        (RETRY_BASE_MS << shift).min(RETRY_MAX_MS)
    }

    /// Re-queue everything left in flight after a restart or reconnect; a
    /// crashed process may have left ops marked in-flight.
    pub fn reset_in_flight(&mut self) {
        for e in self.queue.iter_mut() {
            if e.state == OpState::InFlight {
                e.state = OpState::Pending;
                e.next_attempt_at = e.queued_at;
            }
        }
    }

    /// Drop all ops that require an active relation (after dissolution).
    pub fn drop_relation_ops(&mut self) {
        self.queue.retain(|e| !e.op.requires_relation());
    }

    pub fn to_json(&self) -> String {
        let entries: Vec<&OutboxEntry> = self.queue.iter().collect();
        serde_json::to_string(&entries).expect("outbox serialization cannot fail")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let entries: Vec<OutboxEntry> = serde_json::from_str(json)?;
        let mut outbox = Self::new(entries.len().max(64));
        for e in entries {
            outbox.queue.push_back(e);
        }
        outbox.reset_in_flight();
        Ok(outbox)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: UnixMs = 1_700_000_000_000;

    fn msg(client_id: &str) -> SyncOp {
        SyncOp::SendMessage {
            client_id: client_id.into(),
            body: "hi".into(),
        }
    }

    #[test]
    fn enqueue_take_complete_round_trip() {
        let mut outbox = Outbox::new(8);
        assert_eq!(outbox.enqueue(msg("c1"), T0, None), EnqueueOutcome::Queued);
        // duplicate key is ignored
        assert_eq!(
            outbox.enqueue(msg("c1"), T0, None),
            EnqueueOutcome::Duplicate
        );
        let drained = outbox.take_ready(T0).unwrap();
        assert_eq!(drained.entry.attempts, 1);
        outbox.complete(&msg("c1"), CompleteOutcome::Done, T0);
        assert!(outbox.is_empty());
    }

    #[test]
    fn retry_backoff_and_attempts() {
        let mut outbox = Outbox::new(8);
        outbox.enqueue(msg("c1"), T0, None);
        let _drained = outbox.take_ready(T0).unwrap();
        outbox.complete(
            &msg("c1"),
            CompleteOutcome::Retry {
                after_ms: Outbox::backoff_ms(1),
            },
            T0,
        );
        assert_eq!(Outbox::backoff_ms(1), 2000);
        assert_eq!(Outbox::backoff_ms(2), 4000);
        assert_eq!(Outbox::backoff_ms(8), 256_000);
        assert_eq!(Outbox::backoff_ms(9), RETRY_MAX_MS);
        assert_eq!(Outbox::backoff_ms(50), RETRY_MAX_MS);
        assert!(outbox.take_ready(T0).is_none(), "must wait for backoff");
        assert!(outbox.take_ready(T0 + 1999).is_none());
        let again = outbox.take_ready(T0 + 2000).unwrap();
        assert_eq!(again.entry.attempts, 2);
    }

    #[test]
    fn failed_op_stops_after_max_attempts() {
        let mut outbox = Outbox::new(8);
        outbox.enqueue(msg("c1"), T0, None);
        for attempt in 1..=MAX_ATTEMPTS as i64 {
            let now = T0 + attempt * 1000 * 1000;
            let drained = outbox
                .take_ready(now)
                .unwrap_or_else(|| panic!("attempt {attempt} should drain"));
            assert_eq!(drained.entry.attempts as i64, attempt);
            outbox.complete(&msg("c1"), CompleteOutcome::Retry { after_ms: 1 }, now);
        }
        assert!(
            outbox.take_ready(T0 + 1000 * 1000 * 100).is_none(),
            "op must stop retrying after {MAX_ATTEMPTS} attempts"
        );
    }

    #[test]
    fn poke_cooldown_enforced_locally() {
        let mut outbox = Outbox::new(8);
        let poke = SyncOp::SendPoke {
            client_id: "p1".into(),
        };
        assert_eq!(
            outbox.enqueue(poke.clone(), T0, None),
            EnqueueOutcome::Queued
        );
        // a queued-but-unsent poke blocks a second one
        assert_eq!(
            outbox.enqueue(poke.clone(), T0 + 500, None),
            EnqueueOutcome::Rejected("poke_pending")
        );
        let drained = outbox.take_ready(T0).expect("poke ready");
        assert_eq!(drained.entry.attempts, 1);
        outbox.complete(&poke, CompleteOutcome::Done, T0);
        // after a completed send the server cooldown applies locally
        assert_eq!(
            outbox.enqueue(poke.clone(), T0 + 1000, Some(T0)),
            EnqueueOutcome::Rejected("poke_cooldown")
        );
        assert_eq!(
            outbox.enqueue(poke, T0 + limits::POKE_COOLDOWN_MS, Some(T0)),
            EnqueueOutcome::Queued
        );
    }

    #[test]
    fn stale_pokes_expire_without_replay() {
        let mut outbox = Outbox::new(8);
        let poke = SyncOp::SendPoke {
            client_id: "p1".into(),
        };
        outbox.enqueue(poke, T0, None);
        let late = T0 + limits::POKE_OUTBOX_TTL_MS + 1;
        assert!(outbox.take_ready(late).is_none(), "stale poke must expire");
    }

    #[test]
    fn dissolution_drops_relation_ops() {
        let mut outbox = Outbox::new(8);
        outbox.enqueue(msg("c1"), T0, None);
        outbox.enqueue(
            SyncOp::SetPresence {
                state: crate::PresenceState::Away,
            },
            T0,
            None,
        );
        outbox.drop_relation_ops();
        let left: Vec<_> = outbox.entries().map(|e| e.op.describe()).collect();
        assert_eq!(left, vec!["set_presence"]);
    }

    #[test]
    fn survives_json_round_trip_and_clears_in_flight() {
        let mut outbox = Outbox::new(8);
        outbox.enqueue(msg("c1"), T0, None);
        outbox.enqueue(
            SyncOp::SendLetter {
                client_id: "l1".into(),
                body: "miss you".into(),
            },
            T0,
            None,
        );
        let _ = outbox.take_ready(T0).unwrap();
        let json = outbox.to_json();
        let restored = Outbox::from_json(&json).unwrap();
        let states: Vec<_> = restored.entries().map(|e| e.state).collect();
        assert!(states.iter().all(|s| *s == OpState::Pending));
        assert_eq!(restored.len(), 2);
    }

    #[test]
    fn capacity_bounded() {
        let mut outbox = Outbox::new(2);
        assert_eq!(outbox.enqueue(msg("c1"), T0, None), EnqueueOutcome::Queued);
        assert_eq!(outbox.enqueue(msg("c2"), T0, None), EnqueueOutcome::Queued);
        assert_eq!(
            outbox.enqueue(msg("c3"), T0, None),
            EnqueueOutcome::Rejected("outbox_full")
        );
    }
}
