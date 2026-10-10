//! The pet behavior state machine: maps app and network events onto
//! animations, arbitrates priorities through the player, and adds sparse
//! random life actions so the pet feels alive without flickering.

use crate::names;
use crate::player::AnimationPlayer;
use crate::priority;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::StdRng;

/// What the pet is doing as its base loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseActivity {
    Idle,
    Work,
    Sleep,
    Rest,
}

impl BaseActivity {
    pub fn animation(self) -> &'static str {
        match self {
            BaseActivity::Idle => names::IDLE,
            BaseActivity::Work => names::WORK,
            BaseActivity::Sleep => names::SLEEP,
            BaseActivity::Rest => names::REST,
        }
    }
}

/// One requested animation for the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationRequest {
    pub name: String,
    pub priority: u8,
}

impl AnimationRequest {
    fn new(name: &str, priority: u8) -> Self {
        Self {
            name: name.to_string(),
            priority,
        }
    }
}

/// Events that may change what the pet shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PetEvent {
    /// The user clicked their own pet.
    Click,
    /// A poke arrived from the partner.
    Poked,
    /// A realtime message arrived (or the unread badge appeared).
    MessageArrived,
    /// A letter arrived.
    LetterArrived,
    /// A joint-focus invite arrived.
    FocusInviteArrived,
    /// The joint focus session started.
    FocusStarted,
    /// The joint focus session ended (completed or cancelled).
    FocusEnded { celebrate: bool },
    /// The user initiated a poke towards the partner: local feedback.
    PokeSent,
}

/// Behavioral state: base activity plus the random-life scheduler.
pub struct PetBehavior {
    rng: StdRng,
    base: BaseActivity,
    /// Next moment a random life action may fire.
    next_life_action_at: u64,
    /// No two reactions closer than this, regardless of source.
    reaction_cooldown_until: u64,
    /// Random life actions land in this window (milliseconds).
    life_interval: (u64, u64),
}

/// Reactions shorter than this would feel like flicker.
const REACTION_COOLDOWN_MS: u64 = 2500;

impl PetBehavior {
    /// `seed` makes the random schedule deterministic for tests.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            base: BaseActivity::Idle,
            next_life_action_at: 0,
            reaction_cooldown_until: 0,
            life_interval: (25_000, 70_000),
        }
    }

    pub fn base(&self) -> BaseActivity {
        self.base
    }

    /// Changes the base loop (focus started -> work, user asked to sleep...).
    pub fn set_base(&mut self, base: BaseActivity, player: &mut AnimationPlayer, now_ms: u64) {
        self.base = base;
        player.set_base(base.animation());
        player.play_base(now_ms);
    }

    fn schedule_next_life_action(&mut self, now_ms: u64) {
        let (low, high) = self.life_interval;
        let span = high.saturating_sub(low);
        let offset = self.rng.random_range(0..=span);
        self.next_life_action_at = now_ms + low + offset;
    }

    /// Periodic update: maybe fires a small life action (blink/stretch).
    pub fn update(
        &mut self,
        player: &mut AnimationPlayer,
        now_ms: u64,
    ) -> Option<AnimationRequest> {
        if self.next_life_action_at == 0 {
            self.schedule_next_life_action(now_ms);
            return None;
        }
        if now_ms < self.next_life_action_at {
            // one-shot housekeeping: return to base when a reaction ends
            if player.finished(now_ms) && player.active_name() != Some(player.base_animation()) {
                player.play_base(now_ms);
            }
            return None;
        }
        self.schedule_next_life_action(now_ms);
        // life actions only layer on calm bases; sleeping pets just blink
        let request = match self.base {
            BaseActivity::Idle | BaseActivity::Rest => {
                let roll: u8 = self.rng.random_range(0..10);
                if roll < 7 {
                    AnimationRequest::new(names::BLINK, priority::LIFE)
                } else {
                    AnimationRequest::new(names::STRETCH, priority::LIFE)
                }
            }
            BaseActivity::Sleep | BaseActivity::Work => {
                AnimationRequest::new(names::BLINK, priority::LIFE)
            }
        };
        self.apply(request, player, now_ms)
    }

    /// Applies an event to the state machine.
    pub fn handle(
        &mut self,
        event: PetEvent,
        player: &mut AnimationPlayer,
        now_ms: u64,
    ) -> Option<AnimationRequest> {
        let request = match event {
            PetEvent::Click => AnimationRequest::new(names::REACT, priority::INTERACTION),
            PetEvent::PokeSent => AnimationRequest::new(names::WAVE, priority::INTERACTION),
            PetEvent::Poked => AnimationRequest::new(names::POKE_REACT, priority::INTERACTION),
            PetEvent::MessageArrived => {
                AnimationRequest::new(names::MESSAGE_REACT, priority::REACTION)
            }
            PetEvent::LetterArrived => {
                AnimationRequest::new(names::MESSAGE_REACT, priority::REACTION)
            }
            PetEvent::FocusInviteArrived => {
                AnimationRequest::new(names::INVITE_REACT, priority::REACTION)
            }
            PetEvent::FocusStarted => {
                self.set_base(BaseActivity::Work, player, now_ms);
                return None;
            }
            PetEvent::FocusEnded { celebrate } => {
                self.set_base(BaseActivity::Idle, player, now_ms);
                if !celebrate {
                    return None;
                }
                AnimationRequest::new(names::CELEBRATE, priority::REACTION)
            }
        };
        self.apply(request, player, now_ms)
    }

    /// Priority + cooldown arbitration before touching the player.
    fn apply(
        &mut self,
        request: AnimationRequest,
        player: &mut AnimationPlayer,
        now_ms: u64,
    ) -> Option<AnimationRequest> {
        // interactions always pass; reactions wait out the cooldown so a
        // burst of network events never turns into animation strobing
        if request.priority < priority::INTERACTION && now_ms < self.reaction_cooldown_until {
            return None;
        }
        if matches!(
            player.play(&request.name, request.priority, now_ms),
            crate::player::PlayOutcome::Started
        ) {
            if request.priority < priority::INTERACTION {
                self.reaction_cooldown_until = now_ms + REACTION_COOLDOWN_MS;
            }
            Some(request)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{AnimationClip, CharacterSprites};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn sprites() -> Arc<CharacterSprites> {
        let mut animations = HashMap::new();
        for (name, frames, fps, looped) in [
            (names::IDLE, 4u32, 3u32, true),
            (names::BLINK, 4, 8, false),
            (names::WAVE, 6, 6, false),
            (names::SLEEP, 4, 2, true),
            (names::WORK, 4, 3, true),
            (names::STRETCH, 6, 5, false),
            (names::REACT, 4, 8, false),
            (names::POKE_REACT, 6, 8, false),
            (names::MESSAGE_REACT, 4, 6, false),
            (names::INVITE_REACT, 4, 6, false),
            (names::CELEBRATE, 6, 8, false),
        ] {
            animations.insert(
                name.to_string(),
                (
                    AnimationClip {
                        frames: frames as usize,
                        fps,
                        looped,
                    },
                    PathBuf::from(name),
                ),
            );
        }
        Arc::new(CharacterSprites {
            frame_width: 192,
            frame_height: 256,
            animations,
            sheets: HashMap::new(),
        })
    }

    #[test]
    fn click_and_poke_always_react() {
        let mut player = AnimationPlayer::new(sprites());
        let mut behavior = PetBehavior::new(1);
        let request = behavior
            .handle(PetEvent::Click, &mut player, 1000)
            .expect("click reacts");
        assert_eq!(request.name, names::REACT);
        // even during a reaction cooldown, interactions win
        let poked = behavior
            .handle(PetEvent::Poked, &mut player, 1100)
            .expect("poke reacts immediately after click");
        assert_eq!(poked.name, names::POKE_REACT);
    }

    #[test]
    fn reaction_burst_is_throttled() {
        let mut player = AnimationPlayer::new(sprites());
        let mut behavior = PetBehavior::new(2);
        let first = behavior.handle(PetEvent::MessageArrived, &mut player, 0);
        assert!(first.is_some(), "first message reacts");
        let second = behavior.handle(PetEvent::LetterArrived, &mut player, 500);
        assert!(second.is_none(), "burst within the cooldown is dropped");
        // a poke still breaks through
        let poke = behavior.handle(PetEvent::Poked, &mut player, 600);
        assert!(poke.is_some(), "interactions bypass the cooldown");
        // after the cooldown reactions work again
        let later = behavior.handle(
            PetEvent::MessageArrived,
            &mut player,
            600 + REACTION_COOLDOWN_MS,
        );
        assert!(later.is_some());
    }

    #[test]
    fn focus_start_switches_base_to_work_and_back() {
        let mut player = AnimationPlayer::new(sprites());
        let mut behavior = PetBehavior::new(3);
        behavior.handle(PetEvent::FocusStarted, &mut player, 0);
        assert_eq!(behavior.base(), BaseActivity::Work);
        assert_eq!(player.base_animation(), names::WORK);
        behavior.handle(PetEvent::FocusEnded { celebrate: true }, &mut player, 1000);
        assert_eq!(behavior.base(), BaseActivity::Idle);
        assert_eq!(player.active_name(), Some(names::CELEBRATE));
    }

    #[test]
    fn focus_end_without_celebration_returns_to_base_quietly() {
        let mut player = AnimationPlayer::new(sprites());
        let mut behavior = PetBehavior::new(4);
        behavior.handle(PetEvent::FocusStarted, &mut player, 0);
        let request = behavior.handle(PetEvent::FocusEnded { celebrate: false }, &mut player, 1000);
        assert!(request.is_none());
        assert_eq!(player.base_animation(), names::IDLE);
    }

    #[test]
    fn random_life_actions_are_sparse_and_deterministic() {
        let mut player = AnimationPlayer::new(sprites());
        let mut behavior = PetBehavior::new(42);
        behavior.life_interval = (100, 200);
        let mut fires = Vec::new();
        for ms in (0..=2000).step_by(50) {
            if let Some(request) = behavior.update(&mut player, ms) {
                fires.push((ms, request.name));
            }
        }
        assert!(!fires.is_empty(), "life actions happen within the window");
        assert!(fires.len() <= 20, "never a per-tick strobe");
        // same seed -> same schedule
        let mut other_player = AnimationPlayer::new(sprites());
        let mut other = PetBehavior::new(42);
        other.life_interval = (100, 200);
        let mut other_fires = Vec::new();
        for ms in (0..=2000).step_by(50) {
            if let Some(request) = other.update(&mut other_player, ms) {
                other_fires.push((ms, request.name));
            }
        }
        assert_eq!(fires, other_fires);
    }

    #[test]
    fn sleeping_pet_only_blinks() {
        let mut player = AnimationPlayer::new(sprites());
        let mut behavior = PetBehavior::new(7);
        behavior.life_interval = (0, 10);
        behavior.set_base(BaseActivity::Sleep, &mut player, 0);
        let mut seen = Vec::new();
        for ms in (1..=600).step_by(30) {
            behavior.next_life_action_at = 1; // force a life action every tick
            if let Some(request) = behavior.update(&mut player, ms) {
                seen.push(request.name);
            }
        }
        assert!(!seen.is_empty());
        assert!(
            seen.iter().all(|n| n == names::BLINK),
            "sleeping pets never stretch: {seen:?}"
        );
    }
}
