//! The frame player: one active animation with priority arbitration,
//! monotonic-clock driven frame selection, and automatic return to the base
//! loop after one-shots finish.

use crate::library::{AnimationClip, CharacterSprites};
use crate::{names, priority};
use std::sync::Arc;

/// Injectable monotonic clock (milliseconds).
pub trait MonotonicClock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Wall-clock implementation.
#[derive(Debug)]
pub struct SystemMonotonicClock {
    start: std::time::Instant,
}

impl SystemMonotonicClock {
    pub fn start() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

impl MonotonicClock for SystemMonotonicClock {
    fn now_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayOutcome {
    Started,
    /// A higher-or-equal priority one-shot is still running.
    Busy,
    /// The named animation does not exist in this character's manifest.
    Missing,
}

#[derive(Debug)]
struct ActiveAnimation {
    name: String,
    clip: AnimationClip,
    started_at: u64,
    priority: u8,
    one_shot: bool,
}

/// Drives one character. Frame selection is derived from
/// `now - started_at`, never from per-frame mutation, so pause-free
/// playback is deterministic and testable.
pub struct AnimationPlayer {
    sprites: Arc<CharacterSprites>,
    active: Option<ActiveAnimation>,
    base: String,
}

impl AnimationPlayer {
    pub fn new(sprites: Arc<CharacterSprites>) -> Self {
        Self {
            sprites,
            active: None,
            base: names::IDLE.to_string(),
        }
    }

    /// Swaps the character at runtime, restarting the base loop.
    ///
    /// Used by the character setting: the sheets are decoded per character, so
    /// the whole set is replaced and playback restarts from idle rather than
    /// mixing one character's frame index into another's clip.
    pub fn set_sprites(&mut self, sprites: Arc<CharacterSprites>, now_ms: u64) {
        self.sprites = sprites;
        self.active = None;
        self.play_base(now_ms);
    }

    /// The loop animation the player returns to once one-shots finish.
    pub fn base_animation(&self) -> &str {
        &self.base
    }

    pub fn set_base(&mut self, name: &str) {
        self.base = name.to_string();
    }

    /// Requests `name` at `priority`. One-shots cannot be interrupted by
    /// equal/lower priorities; loop animations can be replaced freely.
    pub fn play(&mut self, name: &str, priority: u8, now_ms: u64) -> PlayOutcome {
        let Some(clip) = self.sprites.clip(name) else {
            return PlayOutcome::Missing;
        };
        if let Some(active) = &self.active
            && active.one_shot
            && active.priority > priority
            && !self.finished(now_ms)
        {
            return PlayOutcome::Busy;
        }
        self.active = Some(ActiveAnimation {
            name: name.to_string(),
            clip,
            started_at: now_ms,
            priority,
            one_shot: !clip.looped,
        });
        PlayOutcome::Started
    }

    /// Starts the base loop (used after one-shots and on startup).
    pub fn play_base(&mut self, now_ms: u64) -> bool {
        matches!(
            self.play(&self.base.clone(), priority::BASE, now_ms),
            PlayOutcome::Started
        )
    }

    pub fn active_name(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.name.as_str())
    }

    pub fn active_priority(&self) -> Option<u8> {
        self.active.as_ref().map(|a| a.priority)
    }

    /// True when a one-shot finished (or nothing is active). The caller
    /// should call [`Self::play_base`] to return to the base loop.
    pub fn finished(&self, now_ms: u64) -> bool {
        match &self.active {
            None => true,
            Some(active) => {
                if active.clip.looped {
                    false
                } else {
                    let duration_ms = clip_duration_ms(&active.clip);
                    now_ms.saturating_sub(active.started_at) >= duration_ms
                }
            }
        }
    }

    /// The frame to draw at `now_ms`, with the index derived from elapsed
    /// time and looping semantics. Falls back to the first idle frame when
    /// the manifest is missing the animation.
    pub fn current_frame(&self, now_ms: u64) -> Option<image::RgbaImage> {
        let active = self.active.as_ref()?;
        let duration = clip_duration_ms(&active.clip);
        let mut elapsed = now_ms.saturating_sub(active.started_at);
        if active.clip.looped {
            if duration == 0 {
                elapsed = 0;
            } else {
                elapsed %= duration;
            }
        } else {
            elapsed = elapsed.min(duration.saturating_sub(1));
        }
        let index = frame_index(elapsed, &active.clip);
        // missing sheets fall back to idle so the pet never blanks out
        self.sprites
            .frame(&active.name, index)
            .or_else(|| self.sprites.frame(names::IDLE, 0))
    }
}

/// Pure frame-index calculation: `elapsed * fps / 1000`, wrapped for loops
/// and clamped to the last frame for one-shots.
fn frame_index(elapsed_ms: u64, clip: &AnimationClip) -> usize {
    if clip.fps == 0 || clip.frames == 0 {
        return 0;
    }
    let index = elapsed_ms * clip.fps as u64 / 1000;
    if clip.looped {
        (index % clip.frames as u64) as usize
    } else {
        index.min(clip.frames as u64 - 1) as usize
    }
}

fn clip_duration_ms(clip: &AnimationClip) -> u64 {
    if clip.fps == 0 {
        return u64::MAX;
    }
    clip.frames as u64 * 1000 / clip.fps as u64
}

/// A fixed clock for tests.
pub struct FixedClock(pub u64);

impl MonotonicClock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::AnimationClip;
    use std::collections::HashMap;

    fn sprites() -> Arc<CharacterSprites> {
        let mut animations = HashMap::new();
        animations.insert(
            names::IDLE.to_string(),
            (
                AnimationClip {
                    frames: 4,
                    fps: 3,
                    looped: true,
                },
                std::path::PathBuf::from("idle"),
            ),
        );
        animations.insert(
            names::POKE_REACT.to_string(),
            (
                AnimationClip {
                    frames: 6,
                    fps: 8,
                    looped: false,
                },
                std::path::PathBuf::from("poke"),
            ),
        );
        animations.insert(
            names::MESSAGE_REACT.to_string(),
            (
                AnimationClip {
                    frames: 4,
                    fps: 6,
                    looped: false,
                },
                std::path::PathBuf::from("message"),
            ),
        );
        animations.insert(
            "work".to_string(),
            (
                AnimationClip {
                    frames: 4,
                    fps: 3,
                    looped: true,
                },
                std::path::PathBuf::from("work"),
            ),
        );
        Arc::new(CharacterSprites {
            frame_width: 192,
            frame_height: 256,
            animations,
            sheets: HashMap::new(),
        })
    }

    #[test]
    fn frame_index_follows_elapsed_time_and_wraps() {
        let idle = AnimationClip {
            frames: 4,
            fps: 3,
            looped: true,
        };
        // idle loop: 4 frames @3fps -> 1333ms cycle, wraps cleanly
        assert_eq!(frame_index(0, &idle), 0);
        assert_eq!(frame_index(500, &idle), 1);
        assert_eq!(frame_index(1400, &idle), 0, "1400ms wraps past the cycle");
        assert_eq!(frame_index(1332, &idle), 3);
        let poke = AnimationClip {
            frames: 6,
            fps: 8,
            looped: false,
        };
        // one-shot: 6 frames @8fps -> clamps at the last frame
        assert_eq!(frame_index(0, &poke), 0);
        assert_eq!(frame_index(500, &poke), 4);
        assert_eq!(
            frame_index(9999, &poke),
            5,
            "one-shots hold their final frame"
        );
        let zero = AnimationClip {
            frames: 1,
            fps: 0,
            looped: true,
        };
        assert_eq!(
            frame_index(12345, &zero),
            0,
            "degenerate clips never divide by zero"
        );
    }

    #[test]
    fn one_shot_priority_blocks_lower_requests_until_done() {
        let mut player = AnimationPlayer::new(sprites());
        player.play(names::POKE_REACT, priority::INTERACTION, 0);
        // poke_react: 6 frames @8fps = 750ms
        assert_eq!(
            player.play(names::MESSAGE_REACT, priority::REACTION, 100),
            PlayOutcome::Busy,
            "a poke reaction cannot be interrupted by a message reaction"
        );
        assert_eq!(player.active_name(), Some(names::POKE_REACT));
        // equal priority restarts the one-shot (poking twice reacts twice)
        assert_eq!(
            player.play(names::POKE_REACT, priority::INTERACTION, 100),
            PlayOutcome::Started
        );
        assert_eq!(
            player.play(names::IDLE, priority::BASE, 100),
            PlayOutcome::Busy,
            "base cannot replace a running one-shot"
        );
        // restarted at t=100, so it finishes at 850
        assert!(!player.finished(749));
        assert!(player.finished(850));
    }

    #[test]
    fn one_shots_return_to_base_loop() {
        let mut player = AnimationPlayer::new(sprites());
        player.set_base(names::IDLE);
        player.play_base(0);
        assert_eq!(player.active_name(), Some(names::IDLE));
        player.play(names::MESSAGE_REACT, priority::REACTION, 0);
        // message_react: 4 frames @6fps = 666ms
        assert!(!player.finished(665));
        assert!(player.finished(666));
        assert!(player.play_base(666));
        assert_eq!(player.active_name(), Some(names::IDLE));
    }

    #[test]
    fn missing_animation_reports_missing() {
        let mut player = AnimationPlayer::new(sprites());
        assert_eq!(player.play("nope", priority::LIFE, 0), PlayOutcome::Missing);
        assert_eq!(player.active_name(), None);
    }

    #[test]
    fn loop_animation_can_be_replaced_by_same_priority() {
        let mut player = AnimationPlayer::new(sprites());
        player.play(names::IDLE, priority::BASE, 0);
        player.play(names::IDLE, priority::BASE, 500);
        assert_eq!(player.active_name(), Some(names::IDLE));
        // base swaps always succeed because loops are interruptible
        player.set_base(names::WORK);
        assert!(player.play_base(500));
        assert_eq!(player.active_name(), Some("work"));
    }
}
