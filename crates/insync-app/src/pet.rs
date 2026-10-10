//! Pet window glue: creates the platform window, routes its events into a
//! channel the main loop drains, and pushes animation frames.

use insync_pet_window::{PetWindowConfig, PetWindowDelegate, PetWindowEvent, PetWindowHandle};
use std::sync::mpsc::Sender;

/// Events the app loop cares about, translated from raw window events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetAppEvent {
    Clicked,
    RightClicked,
    Dragged,
}

pub struct PetEventRelay {
    sender: Sender<PetAppEvent>,
}

impl PetWindowDelegate for PetEventRelay {
    fn on_event(&self, event: PetWindowEvent) {
        let app_event = match event {
            PetWindowEvent::Clicked => PetAppEvent::Clicked,
            PetWindowEvent::RightClicked => PetAppEvent::RightClicked,
            PetWindowEvent::Dragged { .. } => PetAppEvent::Dragged,
        };
        let _ = self.sender.send(app_event);
    }
}

pub struct PetWindow {
    pub handle: PetWindowHandle,
    pub events: std::sync::mpsc::Receiver<PetAppEvent>,
}

/// The sprite asset is 192x256 at 4x; the window starts at 100% scale.
pub const INITIAL_WIDTH_PX: u32 = 192;
pub const INITIAL_HEIGHT_PX: u32 = 256;

pub fn initial_device_size(_core: &crate::app::AppCore) -> u32 {
    INITIAL_WIDTH_PX
}

impl PetWindow {
    /// Creates the pet window. On macOS this must run on the main thread.
    /// `width_px`/`height_px` are device pixels for the current scale.
    pub fn create(
        width_px: u32,
        height_px: u32,
        position: Option<(i32, i32)>,
        click_through: bool,
        always_on_top: bool,
    ) -> Result<Self, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        let delegate = std::sync::Arc::new(PetEventRelay { sender: tx });
        let handle = PetWindowHandle::create(
            PetWindowConfig {
                width_px,
                height_px,
                always_on_top,
                click_through,
                position,
            },
            delegate,
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { handle, events: rx })
    }
}
