//! The shared focus-session state machine.
//!
//! The server is the only authority that applies transitions; both sides use
//! the same transition table from this module so the client projection can
//! never disagree with what the server accepts. Every accepted transition
//! bumps `revision`, and clients apply remote state idempotently by
//! comparing revisions.

use crate::{ProtocolError, UnixMs, limits};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusState {
    /// Waiting for the partner to accept or decline.
    Invited,
    /// Counting down; `end_at` is authoritative.
    Active,
    /// Paused by either side; countdown frozen.
    Paused,
    /// Finished (naturally or early) — terminal.
    Completed,
    /// The partner declined — terminal.
    Declined,
    /// Cancelled by the initiator or timed out unaccepted — terminal.
    Cancelled,
}

impl FocusState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            FocusState::Completed | FocusState::Declined | FocusState::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusAction {
    Accept,
    Decline,
    Cancel,
    Pause,
    Resume,
    FinishEarly,
}

/// A joint focus session. `paused_ms` accumulates across pause windows so
/// `end_at` slides forward on every resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusSession {
    pub id: String,
    pub initiator_id: String,
    pub topic: Option<String>,
    pub duration_secs: i64,
    pub state: FocusState,
    pub revision: u64,
    pub invited_at: UnixMs,
    pub accepted_at: Option<UnixMs>,
    pub started_at: Option<UnixMs>,
    pub end_at: Option<UnixMs>,
    pub paused_ms: i64,
    /// Wall time the current pause started; `None` while not paused.
    pub paused_at: Option<UnixMs>,
    pub ended_at: Option<UnixMs>,
    pub ended_early: bool,
}

impl FocusSession {
    pub fn new(
        id: String,
        initiator_id: String,
        topic: Option<String>,
        duration_secs: i64,
        now: UnixMs,
    ) -> Result<Self, ProtocolError> {
        if !(limits::FOCUS_DURATION_MIN..=limits::FOCUS_DURATION_MAX).contains(&duration_secs) {
            return Err(ProtocolError(format!(
                "focus duration must be {}..={} seconds",
                limits::FOCUS_DURATION_MIN,
                limits::FOCUS_DURATION_MAX
            )));
        }
        if let Some(topic) = &topic
            && topic.chars().count() > limits::FOCUS_TOPIC_MAX
        {
            return Err(ProtocolError("focus topic too long".into()));
        }
        Ok(Self {
            id,
            initiator_id,
            topic,
            duration_secs,
            state: FocusState::Invited,
            revision: 1,
            invited_at: now,
            accepted_at: None,
            started_at: None,
            end_at: None,
            paused_ms: 0,
            paused_at: None,
            ended_at: None,
            ended_early: false,
        })
    }

    /// Remaining milliseconds while active; zero once past `end_at`.
    pub fn remaining_ms(&self, now: UnixMs) -> Option<i64> {
        let end = self.end_at?;
        if self.state != FocusState::Active {
            return None;
        }
        Some((end - now).max(0))
    }

    /// The server-side transition table. `actor_is_initiator` distinguishes
    /// who is acting; every accepted transition bumps the revision exactly
    /// once, which makes replays of the same event no-ops on the client.
    pub fn transition(
        &self,
        action: FocusAction,
        actor_is_initiator: bool,
        now: UnixMs,
    ) -> Result<Self, ProtocolError> {
        let mut next = self.clone();
        next.revision = self.revision + 1;
        match (self.state, action) {
            (FocusState::Invited, FocusAction::Accept) => {
                if actor_is_initiator {
                    return Err(ProtocolError("initiator cannot accept own invite".into()));
                }
                next.state = FocusState::Active;
                next.accepted_at = Some(now);
                next.started_at = Some(now);
                next.end_at = Some(now + self.duration_secs * 1000);
                Ok(next)
            }
            (FocusState::Invited, FocusAction::Decline) => {
                if actor_is_initiator {
                    return Err(ProtocolError("only the partner can decline".into()));
                }
                next.state = FocusState::Declined;
                next.ended_at = Some(now);
                Ok(next)
            }
            (FocusState::Invited, FocusAction::Cancel) => {
                if !actor_is_initiator {
                    return Err(ProtocolError(
                        "only the initiator can cancel an invite".into(),
                    ));
                }
                next.state = FocusState::Cancelled;
                next.ended_at = Some(now);
                Ok(next)
            }
            (FocusState::Active, FocusAction::Pause) => {
                next.state = FocusState::Paused;
                next.paused_at = Some(now);
                Ok(next)
            }
            (FocusState::Paused, FocusAction::Resume) => {
                let paused_at = self
                    .paused_at
                    .ok_or_else(|| ProtocolError("paused session without paused_at".into()))?;
                let window = (now - paused_at).max(0);
                next.state = FocusState::Active;
                next.paused_ms = self.paused_ms + window;
                next.end_at = self.end_at.map(|end| end + window);
                next.paused_at = None;
                Ok(next)
            }
            (FocusState::Active, FocusAction::FinishEarly) => {
                next.state = FocusState::Completed;
                next.ended_at = Some(now);
                next.ended_early = true;
                Ok(next)
            }
            (FocusState::Paused, FocusAction::FinishEarly) => {
                next.state = FocusState::Completed;
                next.ended_at = Some(now);
                next.ended_early = true;
                Ok(next)
            }
            (state, _) if state.is_terminal() => {
                Err(ProtocolError("session already finished".into()))
            }
            (state, action) => Err(ProtocolError(format!(
                "invalid transition {state:?} + {action:?}"
            ))),
        }
    }

    /// Applies a remote snapshot when it carries newer information. Older or
    /// equal revisions are ignored (idempotent replay).
    pub fn apply_remote(&mut self, remote: &FocusSession) -> bool {
        if remote.revision > self.revision {
            *self = remote.clone();
            true
        } else {
            false
        }
    }

    /// True when `now` passed `end_at` while the session still looks active.
    /// The server uses this to finalize; the client uses it to render "due".
    pub fn due_for_completion(&self, now: UnixMs) -> bool {
        self.state == FocusState::Active && self.end_at.is_some_and(|end| now >= end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: UnixMs = 1_700_000_000_000;

    fn session() -> FocusSession {
        FocusSession::new("f1".into(), "a".into(), Some("读书".into()), 1500, T0).unwrap()
    }

    #[test]
    fn invite_accept_starts_with_shared_end() {
        let s = session();
        assert_eq!(s.state, FocusState::Invited);
        let started = s.transition(FocusAction::Accept, false, T0 + 1000).unwrap();
        assert_eq!(started.state, FocusState::Active);
        assert_eq!(started.end_at, Some(T0 + 1000 + 1_500_000));
        assert_eq!(started.revision, 2);
    }

    #[test]
    fn initiator_cannot_accept_own_invite() {
        let s = session();
        assert!(s.transition(FocusAction::Accept, true, T0).is_err());
    }

    #[test]
    fn pause_resume_extends_end() {
        let started = session()
            .transition(FocusAction::Accept, false, T0 + 1000)
            .unwrap();
        let end0 = started.end_at.unwrap();
        let paused = started
            .transition(FocusAction::Pause, true, T0 + 60_000)
            .unwrap();
        assert_eq!(paused.state, FocusState::Paused);
        // the pause window lasts 30 s
        let resumed = paused
            .transition(FocusAction::Resume, false, T0 + 90_000)
            .unwrap();
        assert_eq!(resumed.state, FocusState::Active);
        assert_eq!(resumed.end_at, Some(end0 + 30_000));
        assert_eq!(resumed.paused_ms, 30_000);
        assert_eq!(resumed.revision, 4);
    }

    #[test]
    fn finish_early_is_terminal_and_idempotent() {
        let started = session()
            .transition(FocusAction::Accept, false, T0)
            .unwrap();
        let done = started
            .transition(FocusAction::FinishEarly, false, T0 + 5000)
            .unwrap();
        assert_eq!(done.state, FocusState::Completed);
        assert!(done.ended_early);
        assert!(done.transition(FocusAction::Pause, true, T0).is_err());
    }

    #[test]
    fn decline_and_cancel_are_terminal() {
        assert_eq!(
            session()
                .transition(FocusAction::Decline, false, T0)
                .unwrap()
                .state,
            FocusState::Declined
        );
        assert_eq!(
            session()
                .transition(FocusAction::Cancel, true, T0)
                .unwrap()
                .state,
            FocusState::Cancelled
        );
        assert!(
            session()
                .transition(FocusAction::Cancel, false, T0)
                .is_err()
        );
    }

    #[test]
    fn apply_remote_is_revision_gated() {
        let mut local = session();
        let newer = session()
            .transition(FocusAction::Accept, false, T0 + 10)
            .unwrap();
        assert!(local.apply_remote(&newer));
        assert_eq!(local.state, FocusState::Active);
        assert!(!local.apply_remote(&session()));
        assert_eq!(local.state, FocusState::Active);
    }

    #[test]
    fn due_for_completion_and_remaining() {
        let started = session()
            .transition(FocusAction::Accept, false, T0)
            .unwrap();
        assert_eq!(started.remaining_ms(T0 + 1), Some(1_500_000 - 1));
        assert!(started.due_for_completion(T0 + 1_500_000));
        assert!(!started.due_for_completion(T0 + 100));
        let paused = started.transition(FocusAction::Pause, true, T0).unwrap();
        assert_eq!(paused.remaining_ms(T0 + 1_500_000), None);
    }

    #[test]
    fn duration_bounds_enforced() {
        assert!(FocusSession::new("f".into(), "a".into(), None, 30, T0).is_err());
        assert!(FocusSession::new("f".into(), "a".into(), None, 100_000, T0).is_err());
        assert!(FocusSession::new("f".into(), "a".into(), None, 300, T0).is_ok());
    }
}
