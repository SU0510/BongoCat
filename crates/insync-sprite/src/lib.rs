//! InSync's sprite animation layer: manifest loading, the frame player with
//! priority arbitration, and the pet behavior state machine that turns
//! local and network events into animations.
//!
//! Everything is clock- and RNG-injectable so behavior is unit-testable
//! without real time. See docs/insync/sprite-assets.md for the asset format.

pub mod behavior;
pub mod library;
pub mod player;

pub use behavior::{AnimationRequest, BaseActivity, PetBehavior, PetEvent};
pub use library::{AnimationClip, CharacterSprites, SpriteError, SpriteLibrary};
pub use player::{AnimationPlayer, MonotonicClock, SystemMonotonicClock};

/// Canonical animation names shared with the asset manifest.
pub mod names {
    pub const IDLE: &str = "idle";
    pub const BLINK: &str = "blink";
    pub const WAVE: &str = "wave";
    pub const WALK: &str = "walk";
    pub const SLEEP: &str = "sleep";
    pub const WORK: &str = "work";
    pub const STRETCH: &str = "stretch";
    pub const REACT: &str = "react";
    pub const POKE_REACT: &str = "poke_react";
    pub const MESSAGE_REACT: &str = "message_react";
    pub const INVITE_REACT: &str = "invite_react";
    pub const CELEBRATE: &str = "celebrate";
    pub const REST: &str = "rest";
}

/// Animation priority bands. A running one-shot with priority `>=` the
/// incoming request wins; equal or lower requests are dropped instead of
/// producing animation flicker.
pub mod priority {
    /// Looping base states (idle, sleep, work, rest).
    pub const BASE: u8 = 10;
    /// Small life actions layered on the base (blink, stretch).
    pub const LIFE: u8 = 20;
    /// Reactions to network events (message, invite).
    pub const REACTION: u8 = 30;
    /// Direct user interaction (click, poke) — the strongest response.
    pub const INTERACTION: u8 = 40;
}
