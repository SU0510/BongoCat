//! The InSync desk-pet window: a transparent, borderless, always-on-top
//! window that blits premultiplied RGBA frames. Platform backends live in
//! `macos` (NSPanel + CALayer) and `windows` (layered window +
//! UpdateLayeredWindow); `unsafe` is confined to those two modules per
//! ADR-0090 and every non-trivial block documents its caller invariants.
//!
//! Geometry contract: window size and positions are **device pixels** on
//! Windows and **AppKit screen points** on macOS. The application owns
//! persistence, clamping and scale.

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

use image::RgbaImage;

/// Window configuration at creation time.
#[derive(Debug, Clone)]
pub struct PetWindowConfig {
    /// Initial width in device pixels.
    pub width_px: u32,
    /// Initial height in device pixels.
    pub height_px: u32,
    pub always_on_top: bool,
    pub click_through: bool,
    /// Platform-native initial position (see crate docs).
    pub position: Option<(i32, i32)>,
}

impl Default for PetWindowConfig {
    fn default() -> Self {
        Self {
            width_px: 192,
            height_px: 256,
            always_on_top: true,
            click_through: false,
            position: None,
        }
    }
}

/// Events the window reports to the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetWindowEvent {
    /// Left click without a drag (the pet should react).
    Clicked,
    /// Right click (context menu).
    RightClicked,
    /// A native drag settled at the given position.
    Dragged { x: i32, y: i32 },
}

/// Application-side receiver for window events. Called on the platform's
/// window thread (macOS: main thread; Windows: the pet window's own
/// message thread), so implementations must hand events off internally
/// rather than blocking.
pub trait PetWindowDelegate: Send + Sync {
    fn on_event(&self, event: PetWindowEvent);
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PetWindowError {
    #[error("pet window must be created on the platform main thread")]
    WrongThread,
    #[error("window creation failed: {0}")]
    CreationFailed(String),
    #[error("window already closed")]
    Closed,
}

/// A created window. `Drop` closes it; all methods are safe to call from
/// any thread and queue work to the window's thread.
pub struct PetWindowHandle {
    inner: BackendHandle,
}

#[cfg(target_os = "macos")]
type BackendHandle = macos::MacPetWindow;
#[cfg(target_os = "windows")]
type BackendHandle = windows::WindowsPetWindow;

impl PetWindowHandle {
    /// Creates the window. macOS requires the main thread; Windows spawns
    /// its own window thread.
    pub fn create(
        config: PetWindowConfig,
        delegate: std::sync::Arc<dyn PetWindowDelegate>,
    ) -> Result<Self, PetWindowError> {
        Ok(Self {
            inner: BackendHandle::create(config, delegate)?,
        })
    }

    /// Replaces the displayed frame. An image larger or smaller than the
    /// window resizes it.
    pub fn set_frame(&self, frame: RgbaImage) {
        self.inner.set_frame(frame);
    }

    pub fn set_click_through(&self, click_through: bool) {
        self.inner.set_click_through(click_through);
    }

    pub fn set_always_on_top(&self, always_on_top: bool) {
        self.inner.set_always_on_top(always_on_top);
    }

    pub fn set_visible(&self, visible: bool) {
        self.inner.set_visible(visible);
    }

    pub fn set_position(&self, x: i32, y: i32) {
        self.inner.set_position(x, y);
    }

    /// The current top-left position in platform coordinates.
    pub fn position(&self) -> Option<(i32, i32)> {
        self.inner.position()
    }

    /// The platform backing scale (device pixels per point on macOS, per
    /// CSS pixel on Windows) so the app can size the pet correctly.
    pub fn backing_scale(&self) -> f32 {
        self.inner.backing_scale()
    }

    /// macOS only: applies queued commands on the main thread. The
    /// application calls this once per frame tick. Windows applies commands
    /// directly on its window thread, so this is a no-op there.
    pub fn apply_pending(&self) {
        #[cfg(target_os = "macos")]
        self.inner.apply_pending();
        #[cfg(target_os = "windows")]
        let _ = &self.inner;
    }
}

/// Shared drag threshold: a release within 3 device pixels of the press is
/// a click, anything else was a native drag.
pub const DRAG_THRESHOLD_PX: f64 = 3.0;
