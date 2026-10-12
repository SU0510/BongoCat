//! macOS backend: a borderless nonactivating `NSPanel` with a `CALayer`
//! whose contents are replaced with a `CGImage` per animation frame. The
//! panel setup mirrors the values proven by `bongocat-overlay` (level,
//! collection behavior, transparency), minus the Metal surface.
//!
//! Hit testing is per pixel, the way a layered window behaves on Windows.
//! AppKit only has all-or-nothing `setIgnoresMouseEvents`, so the panel flips
//! that flag from the frame's own alpha (see `HitTest`) and the content view
//! then checks each press against the same alpha: the pet only reacts to
//! clicks on pixels it is actually drawn on, and only a press that starts
//! there can move it. Everywhere else the click reaches whatever is behind
//! the window.
//!
//! Thread model: all ObjC work happens on the main thread. Methods callable
//! from any thread enqueue [`Command`]s; the application calls
//! [`MacPetWindow::apply_pending`] once per frame tick on the main thread
//! (the same "main-thread signals" pattern the BongoCat app uses). Events
//! are delivered to the delegate on the main thread.

use crate::{
    DRAG_THRESHOLD_PX, PetWindowConfig, PetWindowDelegate, PetWindowError, PetWindowEvent,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::runtime::NSObjectProtocol;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSColor, NSEvent, NSPanel, NSScreen, NSView,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CFData;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage, CGImageAlphaInfo,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use objc2_quartz_core::CALayer;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// content view: click / drag / context-menu reporting
// ---------------------------------------------------------------------------

struct PetViewIvars {
    delegate: Arc<dyn PetWindowDelegate>,
    hit: Arc<Mutex<HitTest>>,
    /// Where the button went down, in window points: the click/drag
    /// threshold is measured against it. `None` while no press that started
    /// on the pet is in flight.
    press_location: Mutex<Option<(f64, f64)>>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[ivars = PetViewIvars]
    struct PetView;

    unsafe impl NSObjectProtocol for PetView {}

    impl PetView {
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &objc2_app_kit::NSEvent) {
            let location = event.locationInWindow();
            let on_pet = self.press_is_on_pet(location);
            // Dragging stays AppKit's own background drag, but whether the
            // window may take part is decided for each press: one that starts
            // on a transparent pixel must not move the pet, and every press
            // re-decides, so a gesture that ends without reaching us cannot
            // leave the window stuck.
            if let Some(window) = self.window() {
                window.setMovableByWindowBackground(on_pet);
            }
            if !on_pet {
                // A press on a transparent pixel is not the pet's: nothing is
                // recorded, so the release cannot turn it into a click.
                return;
            }
            *self
                .ivars()
                .press_location
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some((location.x, location.y));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &objc2_app_kit::NSEvent) {
            let location = event.locationInWindow();
            let start = self
                .ivars()
                .press_location
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some((sx, sy)) = start else { return };
            let moved = ((location.x - sx).powi(2) + (location.y - sy).powi(2)).sqrt();
            if moved <= DRAG_THRESHOLD_PX {
                self.ivars().delegate.on_event(PetWindowEvent::Clicked);
            } else if let Some(position) = window_position(self) {
                self.ivars().delegate.on_event(PetWindowEvent::Dragged {
                    x: position.0,
                    y: position.1,
                });
            }
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &objc2_app_kit::NSEvent) {
            if !self.press_is_on_pet(event.locationInWindow()) {
                return;
            }
            self.ivars().delegate.on_event(PetWindowEvent::RightClicked);
        }
    }
);

impl PetView {
    fn new(
        mtm: MainThreadMarker,
        delegate: Arc<dyn PetWindowDelegate>,
        hit: Arc<Mutex<HitTest>>,
    ) -> Retained<Self> {
        let this = <Self as MainThreadOnly>::alloc(mtm).set_ivars(PetViewIvars {
            delegate,
            hit,
            press_location: Mutex::new(None),
        });
        // SAFETY: `this` is a partially-initialized NSView subclass whose
        // ivars were just set; `initWithFrame:` is the designated initializer
        // and returns a fully-owned instance.
        let initialized: Retained<Self> =
            unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        initialized
    }

    /// Whether a point in window coordinates lands on a drawn pixel of the
    /// frame on screen.
    fn press_is_on_pet(&self, location: NSPoint) -> bool {
        let Some(window) = self.window() else {
            return false;
        };
        self.ivars()
            .hit
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .point_on_pet(window.frame(), location.x, location.y)
    }
}

/// Reads the window frame in AppKit screen coordinates (bottom-left based)
/// and converts it to the crate's top-left global form.
fn window_position(view: &PetView) -> Option<(i32, i32)> {
    let mtm = MainThreadMarker::new()?;
    let window = view.window()?;
    let frame = window.frame();
    let primary_height = NSScreenHelper::primary_height(mtm)?;
    // AppKit: origin is bottom-left of the primary screen's coordinate space.
    let top_left_y = primary_height - frame.origin.y - frame.size.height;
    Some((frame.origin.x as i32, top_left_y as i32))
}

struct NSScreenHelper;

impl NSScreenHelper {
    /// The primary screen's height, which anchors AppKit's global
    /// coordinate system.
    fn primary_height(mtm: MainThreadMarker) -> Option<f64> {
        let screens = NSScreen::screens(mtm);
        let first = screens.firstObject()?;
        Some(first.frame().size.height)
    }
}

// ---------------------------------------------------------------------------
// per-pixel hit testing
// ---------------------------------------------------------------------------

/// One frame's alpha silhouette: its size, one alpha byte per pixel in
/// row-major order, and the inclusive bounding box of the drawn pixels
/// (`None` when nothing is drawn).
struct Silhouette {
    width: usize,
    height: usize,
    alpha: Vec<u8>,
    bounds: Option<(usize, usize, usize, usize)>,
}

impl Silhouette {
    /// Reads a frame's alpha channel and its bounding box in a single pass.
    fn of(frame: &image::RgbaImage) -> Self {
        let width = frame.width() as usize;
        let height = frame.height() as usize;
        let mut alpha = Vec::with_capacity(width * height);
        let (mut min_x, mut max_x) = (usize::MAX, 0);
        let (mut min_y, mut max_y) = (usize::MAX, 0);
        for (y, row) in frame.rows().enumerate() {
            let mut row_min = usize::MAX;
            let mut row_max = 0;
            for (x, pixel) in row.enumerate() {
                let a = pixel.0[3];
                if a > 0 {
                    row_min = row_min.min(x);
                    row_max = x;
                }
                alpha.push(a);
            }
            if row_min != usize::MAX {
                // rows arrive in order, so the first inked row is the top of
                // the box and each later one moves the bottom
                if min_y == usize::MAX {
                    min_y = y;
                }
                max_y = y;
                min_x = min_x.min(row_min);
                max_x = max_x.max(row_max);
            }
        }
        Self {
            width,
            height,
            alpha,
            bounds: (min_y != usize::MAX).then_some((min_x, min_y, max_x, max_y)),
        }
    }
}

/// The alpha silhouette of the frame on screen, plus the mouse policy
/// derived from it and the cursor.
///
/// Two different questions are answered from the same alpha data. The window
/// only has all-or-nothing `setIgnoresMouseEvents`, so that flag follows the
/// *bounding box* of the drawn pixels: a click aimed at the pet is never lost
/// to the sampling delay between frame ticks. Whether the pet reacts at all
/// is then decided per pixel from the view's own hit test, so a press on a
/// transparent pixel does not reach the pet at all.
struct HitTest {
    /// The frame currently on screen; `None` until the first one arrives.
    mask: Option<Silhouette>,
    /// The user's "click through" setting: the panel never takes the mouse.
    click_through: bool,
    /// The `ignoresMouseEvents` value currently applied to the panel.
    ignoring: bool,
}

impl HitTest {
    fn new(click_through: bool) -> Self {
        Self {
            mask: None,
            click_through,
            ignoring: click_through,
        }
    }

    /// Maps a point in the panel's own coordinates (x from the left, y from
    /// the bottom, both in points) to the image pixel under it.
    ///
    /// `NSEvent::mouseLocation` and the panel frame share AppKit's screen
    /// space, while image rows are counted from the top. The panel is sized
    /// to image pixels / backing scale, so the pixels-per-point ratio is
    /// derived from the frame itself and stays right whatever density the
    /// layer ends up drawing at.
    fn pixel_at(&self, frame: NSRect, x_pt: f64, y_from_bottom: f64) -> Option<(usize, usize)> {
        let mask = self.mask.as_ref()?;
        let (width, height) = (mask.width as f64, mask.height as f64);
        if mask.width == 0
            || mask.height == 0
            || frame.size.width <= 0.0
            || frame.size.height <= 0.0
        {
            return None;
        }
        // Outside the frame there is no pixel: a point beyond an edge must not
        // wrap into the opposite row or column, which a saturating cast to
        // `usize` would otherwise do for negative or oversized values.
        if x_pt < 0.0 || y_from_bottom < 0.0 {
            return None;
        }
        if x_pt >= frame.size.width || y_from_bottom >= frame.size.height {
            return None;
        }
        let px = (x_pt * width / frame.size.width) as usize;
        let py = ((frame.size.height - y_from_bottom) * height / frame.size.height) as usize;
        if px >= mask.width || py >= mask.height {
            return None;
        }
        Some((px, py))
    }

    /// Whether a point on the panel is on a drawn pixel — the exact
    /// silhouette, which is what decides if the pet reacts to a click.
    fn point_on_pet(&self, frame: NSRect, x_pt: f64, y_from_bottom: f64) -> bool {
        let Some((px, py)) = self.pixel_at(frame, x_pt, y_from_bottom) else {
            return false;
        };
        let Some(mask) = self.mask.as_ref() else {
            return false;
        };
        mask.alpha[py * mask.width + px] > 0
    }

    /// Whether the panel should take the mouse at all: the cursor has to be
    /// inside the bounding box of the drawn pixels. Outside it — the
    /// transparent margins of the frame, another screen, or a frame with
    /// nothing drawn on it — the pet yields the mouse to whatever is behind
    /// it, as a layered window does on Windows.
    fn cursor_on_pet(&self, frame: NSRect, mouse: NSPoint) -> bool {
        let Some(mask) = self.mask.as_ref() else {
            return false;
        };
        let Some((min_x, min_y, max_x, max_y)) = mask.bounds else {
            return false;
        };
        let Some((px, py)) =
            self.pixel_at(frame, mouse.x - frame.origin.x, mouse.y - frame.origin.y)
        else {
            return false;
        };
        px >= min_x && px <= max_x && py >= min_y && py <= max_y
    }
}

// ---------------------------------------------------------------------------
// commands
// ---------------------------------------------------------------------------

enum Command {
    Frame(image::RgbaImage),
    ClickThrough(bool),
    AlwaysOnTop(bool),
    Visible(bool),
    Position(i32, i32),
    Close,
}

// ---------------------------------------------------------------------------
// window
// ---------------------------------------------------------------------------

struct SharedGeometry {
    position: Option<(i32, i32)>,
    backing_scale: f32,
}

pub struct MacPetWindow {
    panel: Retained<NSPanel>,
    layer: Retained<CALayer>,
    geometry: Arc<Mutex<SharedGeometry>>,
    pending: Arc<Mutex<VecDeque<Command>>>,
    /// Shared with the content view, which hit-tests presses against it.
    hit_test: Arc<Mutex<HitTest>>,
}

impl MacPetWindow {
    pub fn create(
        config: PetWindowConfig,
        delegate: Arc<dyn PetWindowDelegate>,
    ) -> Result<Self, PetWindowError> {
        let Some(mtm) = MainThreadMarker::new() else {
            return Err(PetWindowError::WrongThread);
        };
        // Accessory policy keeps the app out of the Dock and the panel out
        // of the menu bar; the pet never takes key focus.
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);

        let height_for_coords = NSScreenHelper::primary_height(mtm).unwrap_or(1080.0);
        let (x, y) = config.position.unwrap_or((100, 200));
        // top-left global -> AppKit bottom-left origin
        let origin_y = height_for_coords - f64::from(y) - f64::from(config.height_px);
        let frame = NSRect::new(
            NSPoint::new(f64::from(x), origin_y),
            NSSize::new(f64::from(config.width_px), f64::from(config.height_px)),
        );

        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            <NSPanel as MainThreadOnly>::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        panel.setOpaque(false);
        panel.setHasShadow(false);
        panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
        // `setOpaque(false)` only tells AppKit that the window is not opaque;
        // it still fills the frame with `backgroundColor`, which defaults to
        // the opaque `windowBackgroundColor`. Without this the pet stands on a
        // solid rectangle.
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setLevel(window_level(config.always_on_top));
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        // AppKit's own background drag moves the panel; the content view turns
        // it off again for every press that does not start on the pet
        panel.setMovableByWindowBackground(true);
        panel.setIgnoresMouseEvents(config.click_through);
        unsafe { panel.setReleasedWhenClosed(false) };

        let hit_test = Arc::new(Mutex::new(HitTest::new(config.click_through)));
        let view = PetView::new(mtm, delegate, Arc::clone(&hit_test));
        view.setWantsLayer(true);
        let layer = CALayer::new();
        layer.setOpaque(false);
        view.setLayer(Some(&layer));
        panel.setContentView(Some(&view));

        let geometry = Arc::new(Mutex::new(SharedGeometry {
            position: Some((x, y)),
            backing_scale: panel.backingScaleFactor() as f32,
        }));
        panel.orderFront(None);
        Ok(Self {
            panel,
            layer,
            geometry,
            pending: Arc::new(Mutex::new(VecDeque::new())),
            hit_test,
        })
    }

    fn enqueue(&self, command: Command) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(command);
    }

    /// Drains and applies every queued command, then re-derives the panel's
    /// mouse policy from the frame now on screen. Must run on the main
    /// thread; the application calls this once per frame tick.
    pub fn apply_pending(&self) {
        if MainThreadMarker::new().is_none() {
            return;
        }
        let commands: Vec<Command> = {
            let mut pending = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            pending.drain(..).collect()
        };
        for command in commands {
            self.apply(command);
        }
        self.refresh_hit_test();
    }

    fn apply(&self, command: Command) {
        match command {
            Command::Frame(frame) => self.apply_frame(frame),
            Command::ClickThrough(click_through) => {
                self.hit_test
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .click_through = click_through;
                self.apply_mouse_policy();
            }
            Command::AlwaysOnTop(always_on_top) => {
                self.panel.setLevel(window_level(always_on_top));
                if always_on_top {
                    self.panel.orderFront(None);
                }
            }
            Command::Visible(visible) => {
                if visible {
                    self.panel.orderFront(None);
                } else {
                    self.panel.orderOut(None);
                }
            }
            Command::Position(x, y) => {
                let Some(mtm) = MainThreadMarker::new() else {
                    return;
                };
                let height_for_coords = NSScreenHelper::primary_height(mtm).unwrap_or(1080.0);
                let frame = self.panel.frame();
                let origin_y = height_for_coords - f64::from(y) - frame.size.height;
                self.panel.setFrame_display(
                    NSRect::new(NSPoint::new(f64::from(x), origin_y), frame.size),
                    false,
                );
                self.geometry
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .position = Some((x, y));
            }
            Command::Close => {
                self.panel.orderOut(None);
                self.panel.close();
            }
        }
    }

    fn apply_frame(&self, frame: image::RgbaImage) {
        let (width, height) = (frame.width(), frame.height());
        self.store_mask(&frame);
        let Ok(image) = make_cg_image(frame) else {
            return;
        };
        let scale = self.panel.backingScaleFactor() as f32;
        self.geometry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .backing_scale = scale;
        // content size in points = device pixels / backing scale
        let content_width = f64::from(width) / f64::from(scale);
        let content_height = f64::from(height) / f64::from(scale);
        let current = self.panel.frame();
        if (current.size.width - content_width).abs() > 0.5
            || (current.size.height - content_height).abs() > 0.5
        {
            let origin = current.origin;
            // keep the top edge anchored while resizing
            self.panel.setFrame_display(
                NSRect::new(
                    NSPoint::new(origin.x, origin.y + current.size.height - content_height),
                    NSSize::new(content_width, content_height),
                ),
                false,
            );
        }
        // SAFETY: `image` is a valid CGImage we just created.
        unsafe { self.layer.setContents(Some(image.any_object_ref())) };
    }

    /// Records the alpha silhouette of the frame that is about to go on
    /// screen — the data both halves of the hit test run on.
    fn store_mask(&self, frame: &image::RgbaImage) {
        let silhouette = Silhouette::of(frame);
        self.hit_test
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .mask = Some(silhouette);
    }

    /// Re-derives the panel's mouse policy from the frame's alpha and the
    /// cursor position. Main thread, once per frame tick.
    fn refresh_hit_test(&self) {
        // A held button may be mid-drag: the window drag is running and the
        // release still has to reach the view, so the policy waits for the
        // last button to come up.
        if NSEvent::pressedMouseButtons() != 0 {
            return;
        }
        self.apply_mouse_policy();
    }

    /// Applies "the panel takes the mouse only over the pet" to the panel.
    fn apply_mouse_policy(&self) {
        let mut hit = self
            .hit_test
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ignoring =
            hit.click_through || !hit.cursor_on_pet(self.panel.frame(), NSEvent::mouseLocation());
        if ignoring != hit.ignoring {
            hit.ignoring = ignoring;
            self.panel.setIgnoresMouseEvents(ignoring);
        }
    }

    pub fn position(&self) -> Option<(i32, i32)> {
        self.geometry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .position
    }

    pub fn backing_scale(&self) -> f32 {
        self.geometry
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .backing_scale
    }
}

impl MacPetWindow {
    pub fn set_frame(&self, frame: image::RgbaImage) {
        self.enqueue(Command::Frame(frame));
    }

    pub fn set_click_through(&self, click_through: bool) {
        self.enqueue(Command::ClickThrough(click_through));
    }

    pub fn set_always_on_top(&self, always_on_top: bool) {
        self.enqueue(Command::AlwaysOnTop(always_on_top));
    }

    pub fn set_visible(&self, visible: bool) {
        self.enqueue(Command::Visible(visible));
    }

    pub fn set_position(&self, x: i32, y: i32) {
        self.enqueue(Command::Position(x, y));
    }
}

impl Drop for MacPetWindow {
    fn drop(&mut self) {
        self.enqueue(Command::Close);
        // apply immediately when we happen to be on the main thread (the
        // common shutdown path); otherwise the app's final apply_pending
        // on the main thread closes the panel
        self.apply_pending();
    }
}

fn window_level(always_on_top: bool) -> isize {
    if always_on_top {
        // NSFloatingWindowLevel: above normal windows, below the menu bar
        3
    } else {
        0
    }
}

/// Wraps an RGBA bitmap into a `CGImage`. The bitmap's binary alpha is
/// already premultiplied, so no conversion is needed.
fn make_cg_image(frame: image::RgbaImage) -> Result<CFRetained<CGImage>, ()> {
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    if width == 0 || height == 0 {
        return Err(());
    }
    let bytes_per_row = width * 4;
    let pixels = frame.into_raw();
    // SAFETY: `CFData::new` copies the buffer; `pixels` outlives the call.
    let data = unsafe { CFData::new(None, pixels.as_ptr(), pixels.len() as isize) }.ok_or(())?;
    let provider = CGDataProvider::with_cf_data(Some(&data)).ok_or(())?;
    let colorspace = CGColorSpace::new_device_rgb().ok_or(())?;
    let bitmap_info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);
    // SAFETY: all parameters describe the `pixels` buffer exactly; the
    // provider owns a copy of the bytes.
    unsafe {
        CGImage::new(
            width,
            height,
            8,
            32,
            bytes_per_row,
            Some(&colorspace),
            bitmap_info,
            Some(&provider),
            std::ptr::null(),
            false,
            CGColorRenderingIntent(0), // kCGRenderingIntentDefault
        )
    }
    .ok_or(())
}

/// Bridging helper: a `CGImage` is an opaque CF object and `CALayer`'s
/// contents accepts any object pointer, so the pointer reinterpret is
/// representation-preserving. The layer retains the image.
trait AnyRef {
    fn any_object_ref(&self) -> &AnyObject;
}

impl AnyRef for CGImage {
    fn any_object_ref(&self) -> &AnyObject {
        // SAFETY: CFTypeRef and Objective-C object pointers share the same
        // representation; the caller keeps the `Retained<CGImage>` alive
        // through the `setContents` call and the layer retains it after.
        unsafe { &*(std::ptr::from_ref::<CGImage>(self).cast::<AnyObject>()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn frame(size: (u32, u32), drawn: &[(u32, u32)]) -> image::RgbaImage {
        let mut frame = image::RgbaImage::new(size.0, size.1);
        for &(x, y) in drawn {
            frame.put_pixel(x, y, Rgba([10, 20, 30, 255]));
        }
        frame
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
        NSRect::new(NSPoint::new(x, y), NSSize::new(width, height))
    }

    fn hit_test(frame: &image::RgbaImage) -> HitTest {
        let mut hit = HitTest::new(false);
        hit.mask = Some(Silhouette::of(frame));
        hit
    }

    #[test]
    fn silhouette_keeps_every_alpha_and_the_box_around_the_drawn_pixels() {
        let silhouette = Silhouette::of(&frame((4, 3), &[(1, 0), (3, 2)]));
        assert_eq!((silhouette.width, silhouette.height), (4, 3));
        assert_eq!(silhouette.alpha.len(), 12);
        assert_eq!(silhouette.alpha[1], 255);
        assert_eq!(silhouette.alpha[2], 0);
        assert_eq!(silhouette.alpha[2 * 4 + 3], 255);
        assert_eq!(silhouette.bounds, Some((1, 0, 3, 2)));
    }

    #[test]
    fn an_empty_frame_has_no_box() {
        let silhouette = Silhouette::of(&frame((4, 3), &[]));
        assert_eq!(silhouette.bounds, None);
        assert!(silhouette.alpha.iter().all(|a| *a == 0));
    }

    #[test]
    fn a_point_maps_to_the_pixel_under_it_at_any_backing_scale() {
        let hit = hit_test(&frame((192, 256), &[(0, 0)]));
        // Retina: 192x256 device pixels drawn into a 96x128 point panel
        let retina = rect(100.0, 200.0, 96.0, 128.0);
        assert_eq!(hit.pixel_at(retina, 0.0, 0.5), Some((0, 255)));
        assert_eq!(hit.pixel_at(retina, 48.0, 64.0), Some((96, 128)));
        assert_eq!(hit.pixel_at(retina, 95.9, 127.9), Some((191, 0)));
        // 1x: the panel is the frame in points
        let plain = rect(0.0, 0.0, 192.0, 256.0);
        assert_eq!(hit.pixel_at(plain, 10.0, 10.0), Some((10, 246)));
    }

    #[test]
    fn points_outside_the_frame_and_frames_without_a_mask_have_no_pixel() {
        let hit = hit_test(&frame((192, 256), &[(0, 0)]));
        let retina = rect(100.0, 200.0, 96.0, 128.0);
        assert_eq!(hit.pixel_at(retina, -0.5, 10.0), None);
        assert_eq!(hit.pixel_at(retina, 10.0, -0.5), None);
        // past an edge: must not wrap around into the opposite edge
        assert_eq!(hit.pixel_at(retina, 96.0, 10.0), None);
        assert_eq!(hit.pixel_at(retina, 10.0, 128.0), None);
        assert_eq!(hit.pixel_at(retina, 10.0, 130.0), None);
        assert_eq!(HitTest::new(false).pixel_at(retina, 10.0, 10.0), None);
    }

    #[test]
    fn only_presses_on_drawn_pixels_reach_the_pet() {
        let hit = hit_test(&frame((4, 4), &[(2, 1)]));
        let panel = rect(0.0, 0.0, 4.0, 4.0);
        // the drawn pixel is at x = 2, so its centres sit between 2 and 3 in
        // points, and 4 - 1 - 1 = 2 up from the panel's bottom edge
        assert!(hit.point_on_pet(panel, 2.5, 2.5));
        assert!(!hit.point_on_pet(panel, 0.5, 2.5));
        assert!(!hit.point_on_pet(panel, 3.5, 0.5));
        assert!(!hit.point_on_pet(panel, -1.0, 2.5));
    }

    #[test]
    fn the_panel_takes_the_mouse_only_inside_the_box() {
        let hit = hit_test(&frame((4, 4), &[(2, 1)]));
        let panel = rect(0.0, 0.0, 4.0, 4.0);
        assert!(hit.cursor_on_pet(panel, NSPoint::new(2.5, 2.5)));
        // inside the panel but outside the box around the drawn pixel
        assert!(!hit.cursor_on_pet(panel, NSPoint::new(0.5, 2.5)));
        // off the panel entirely
        assert!(!hit.cursor_on_pet(panel, NSPoint::new(9.0, 9.0)));
        // nothing drawn: the pet never keeps the mouse to itself
        assert!(!hit_test(&frame((4, 4), &[])).cursor_on_pet(panel, NSPoint::new(2.5, 2.5)));
    }
}
