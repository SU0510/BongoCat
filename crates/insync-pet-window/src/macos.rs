//! macOS backend: a borderless nonactivating `NSPanel` with a `CALayer`
//! whose contents are replaced with a `CGImage` per animation frame. The
//! panel setup mirrors the values proven by `bongocat-overlay` (level,
//! collection behavior, transparency), minus the Metal surface.
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
    NSApplication, NSBackingStoreType, NSPanel, NSScreen, NSView, NSWindowCollectionBehavior,
    NSWindowStyleMask,
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
            let mut press = self
                .ivars()
                .press_location
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *press = Some((location.x, location.y));
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
        fn right_mouse_down(&self, _event: &objc2_app_kit::NSEvent) {
            self.ivars().delegate.on_event(PetWindowEvent::RightClicked);
        }
    }
);

impl PetView {
    fn new(mtm: MainThreadMarker, delegate: Arc<dyn PetWindowDelegate>) -> Retained<Self> {
        let this = <Self as MainThreadOnly>::alloc(mtm).set_ivars(PetViewIvars {
            delegate,
            press_location: Mutex::new(None),
        });
        // SAFETY: `this` is a partially-initialized NSView subclass whose
        // ivars were just set; `initWithFrame:` is the designated initializer
        // and returns a fully-owned instance.
        let initialized: Retained<Self> =
            unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] };
        initialized
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
        panel.setLevel(window_level(config.always_on_top));
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        panel.setMovableByWindowBackground(true);
        panel.setIgnoresMouseEvents(config.click_through);
        unsafe { panel.setReleasedWhenClosed(false) };

        let view = PetView::new(mtm, delegate);
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
        })
    }

    fn enqueue(&self, command: Command) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(command);
    }

    /// Drains and applies every queued command. Must run on the main
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
    }

    fn apply(&self, command: Command) {
        match command {
            Command::Frame(frame) => self.apply_frame(frame),
            Command::ClickThrough(click_through) => {
                self.panel.setIgnoresMouseEvents(click_through);
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
