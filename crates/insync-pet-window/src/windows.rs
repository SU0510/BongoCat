//! Windows backend: a `WS_POPUP` layered window updated per frame via
//! `UpdateLayeredWindow` with premultiplied ARGB — the classic desktop-pet
//! path with OS-guaranteed per-pixel alpha and no GPU pipeline.
//!
//! Thread model: the window lives on a dedicated thread running a standard
//! `GetMessageW` loop. Public methods push commands through a channel; the
//! window thread applies them, so no caller ever touches an `HWND`.
//!
//! Geometry contract: sizes and positions are **device pixels**; the
//! process is declared per-monitor DPI-aware by the application manifest
//! so device pixels are stable across screens.

use crate::{
    DRAG_THRESHOLD_PX, PetWindowConfig, PetWindowDelegate, PetWindowError, PetWindowEvent,
};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

const CLASS_NAME: windows::core::PCWSTR = windows::core::w!("InSyncPetWindowClass");

enum Command {
    Frame(image::RgbaImage),
    ClickThrough(bool),
    AlwaysOnTop(bool),
    Visible(bool),
    Position(i32, i32),
    Close,
}

/// State shared between the public handle and the window thread.
struct WindowShared {
    sender: Mutex<Option<Sender<Command>>>,
    position: Mutex<Option<(i32, i32)>>,
    backing_scale: std::sync::atomic::AtomicU32,
}

pub struct WindowsPetWindow {
    shared: Arc<WindowShared>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WindowsPetWindow {
    pub fn create(
        config: PetWindowConfig,
        delegate: Arc<dyn PetWindowDelegate>,
    ) -> Result<Self, PetWindowError> {
        let shared = Arc::new(WindowShared {
            sender: Mutex::new(None),
            position: Mutex::new(config.position),
            backing_scale: std::sync::atomic::AtomicU32::new(f32::to_bits(1.0)),
        });
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("insync-pet-window".to_string())
            .spawn(move || {
                run_window_thread(config, thread_shared, delegate);
            })
            .map_err(|e| PetWindowError::CreationFailed(format!("spawn window thread: {e}")))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    fn send(&self, command: Command) {
        let sender = self
            .shared
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(sender) = sender {
            let _ = sender.send(command);
        }
    }
}

impl WindowsPetWindow {
    pub fn set_frame(&self, frame: image::RgbaImage) {
        self.send(Command::Frame(frame));
    }

    pub fn set_click_through(&self, click_through: bool) {
        self.send(Command::ClickThrough(click_through));
    }

    pub fn set_always_on_top(&self, always_on_top: bool) {
        self.send(Command::AlwaysOnTop(always_on_top));
    }

    pub fn set_visible(&self, visible: bool) {
        self.send(Command::Visible(visible));
    }

    pub fn set_position(&self, x: i32, y: i32) {
        self.send(Command::Position(x, y));
    }

    pub fn position(&self) -> Option<(i32, i32)> {
        *self
            .shared
            .position
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn backing_scale(&self) -> f32 {
        f32::from_bits(
            self.shared
                .backing_scale
                .load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}

impl Drop for WindowsPetWindow {
    fn drop(&mut self) {
        self.send(Command::Close);
        if let Some(thread) = self.thread.take() {
            // the window thread exits after WM_DESTROY; join with a slack
            // budget so a wedged system cannot hang the app at shutdown
            let _ = thread.join();
        }
    }
}

// ---------------------------------------------------------------------------
// window thread (unsafe Win32 boundary)
// ---------------------------------------------------------------------------

/// Per-window state handed to the `WNDPROC` through `GWLP_USERDATA`.
struct WindowThreadState {
    delegate: Arc<dyn PetWindowDelegate>,
    receiver: Receiver<Command>,
    /// The press point in client coordinates for click-vs-drag detection.
    press: Option<(i32, i32)>,
    /// Set while the native caption drag is in flight.
    dragging: bool,
    shared: Arc<WindowShared>,
}

fn run_window_thread(
    config: PetWindowConfig,
    shared: Arc<WindowShared>,
    delegate: Arc<dyn PetWindowDelegate>,
) {
    unsafe { run_window_thread_inner(config, shared, delegate) }
}

/// SAFETY (whole function): classic single-threaded Win32 window setup and
/// pump. The WNDPROC only touches `WindowThreadState` through the
/// `GWLP_USERDATA` pointer installed at creation, the thread owns the loop,
/// and every Win32 call receives pointers the API documents as valid for
/// the duration of the call.
unsafe fn run_window_thread_inner(
    config: PetWindowConfig,
    shared: Arc<WindowShared>,
    delegate: Arc<dyn PetWindowDelegate>,
) {
    // SAFETY: single-threaded Win32 setup and pump; every call below
    // receives pointers the API documents as valid for its duration and the
    // thread exclusively owns the window state (see the module docs).
    unsafe { run_window_thread_body(config, shared, delegate) }
}

/// The actual setup + pump. Its whole body is one `unsafe` block: every
/// call receives parameters the API documents as valid for its duration
/// and the thread exclusively owns the window state.
fn run_window_thread_body(
    config: PetWindowConfig,
    shared: Arc<WindowShared>,
    delegate: Arc<dyn PetWindowDelegate>,
) {
    
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe {
        // SAFETY: process-global module handle; no parameters can be invalid.
        let hinstance = unsafe { GetModuleHandleW(None).unwrap_or_default() };

        // A invisible parent keeps the pet out of Alt-Tab; the popup style with
        // WS_EX_TOOLWINDOW does the same. Register the class once per process.
        static REGISTERED: std::sync::Once = std::sync::Once::new();
        let mut registered_ok = true;
        REGISTERED.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(pet_wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                style: CS_HREDRAW | CS_VREDRAW,
                ..Default::default()
            };
            // SAFETY: `wc` outlives the call and points to process-static names.
            let atom = unsafe { RegisterClassW(&wc) };
            registered_ok = atom != 0;
        });
        if !registered_ok {
            return;
        }

        let (sender, receiver) = std::sync::mpsc::channel::<Command>();
        *shared
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(sender);

        let state = Box::new(WindowThreadState {
            delegate,
            receiver,
            press: None,
            dragging: false,
            shared: Arc::clone(&shared),
        });

        let (x, y) = config.position.unwrap_or((100, 200));
        let mut ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
        if config.click_through {
            ex_style |= WS_EX_TRANSPARENT;
        }
        let hwnd_result = CreateWindowExW(
            ex_style,
            CLASS_NAME,
            windows::core::w!("InSync"),
            WS_POPUP,
            x,
            y,
            config.width_px as i32,
            config.height_px as i32,
            None,
            None,
            Some(hinstance.into()),
            Some(&*state as *const WindowThreadState as _),
        );
        let Ok(hwnd) = hwnd_result else { return };
        // GWLP_USERDATA now owns the Box; recovered by every WNDPROC call.
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        // per-monitor DPI scale for the record (sizes stay device pixels)
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd);
        shared.backing_scale.store(
            f32::to_bits(dpi as f32 / 96.0),
            std::sync::atomic::Ordering::Relaxed,
        );

        if config.always_on_top {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }

        let mut msg = MSG::default();
        'pump: loop {
            // drain queued commands between messages; a frame push must not
            // wait for the next message to arrive
            loop {
                let command = match state_receiver(hwnd) {
                    Some(receiver) => match receiver.try_recv() {
                        Ok(command) => command,
                        Err(_) => break,
                    },
                    None => break,
                };
                if apply_command(hwnd, command) {
                    break 'pump;
                }
            }
            let status = GetMessageW(&mut msg, Some(hwnd), 0, 0);
            if status.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    } // unsafe: whole body
}

/// The receiver is stored in the same `WindowThreadState` the WNDPROC uses;
/// this helper recovers it from the HWND.
unsafe fn state_receiver(
    hwnd: windows::Win32::Foundation::HWND,
) -> Option<&'static Receiver<Command>> {
    // SAFETY: GWLP_USERDATA was installed at window creation and cleared on
    // WM_DESTROY; while a window exists it points at the live thread state.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, GetWindowLongPtrW};
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowThreadState;
        if ptr.is_null() {
            return None;
        }
        Some(&(*ptr).receiver)
    }
}

/// Applies one command; returns true when the loop should exit (Close).
unsafe fn apply_command(hwnd: windows::Win32::Foundation::HWND, command: Command) -> bool {
    // SAFETY: `hwnd` is the live pet window owned by this thread; every
    // call acts on it or a DIB created locally.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::*;
        match command {
            Command::Frame(frame) => {
                let _ = update_layered_frame(hwnd, &frame);
                false
            }
            Command::ClickThrough(on) => {
                toggle_ex_style(hwnd, WS_EX_TRANSPARENT, on);
                false
            }
            Command::AlwaysOnTop(on) => {
                let _ = SetWindowPos(
                    hwnd,
                    Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                false
            }
            Command::Visible(on) => {
                let _ = ShowWindow(hwnd, if on { SW_SHOWNOACTIVATE } else { SW_HIDE });
                false
            }
            Command::Position(x, y) => {
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
                );
                false
            }
            Command::Close => {
                let _ = DestroyWindow(hwnd);
                true
            }
        }
    }
}

unsafe fn toggle_ex_style(
    hwnd: windows::Win32::Foundation::HWND,
    style: windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE,
    on: bool,
) {
    // SAFETY: valid HWND owned by this thread.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW,
        };
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let next = if on {
            current | style.0
        } else {
            current & !style.0
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next as isize);
    }
}

unsafe extern "system" fn pet_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: standard WNDPROC; the HWND is valid for the call and the
    // GWLP_USERDATA state is owned by this thread.
    unsafe {
        use windows::Win32::Foundation::LRESULT;
        use windows::Win32::UI::WindowsAndMessaging::*;
        match msg {
            WM_NCCREATE => {
                // attach the state box carried in lparam (CreateWindowExW's
                // last parameter)
                let create = &*(lparam.0 as *const CREATESTRUCTW);
                let state = create.lpCreateParams as *mut WindowThreadState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
            WM_DESTROY => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowThreadState;
                if !ptr.is_null() {
                    drop(Box::from_raw(ptr));
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_NCHITTEST => LRESULT(HTCLIENT as isize),
            WM_LBUTTONDOWN => {
                let Some(state) = window_state(hwnd) else {
                    return DefWindowProcW(hwnd, msg, wparam, lparam);
                };
                let x = (lparam.0 & 0xFFFF) as i16 as i32;
                let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                state.press = Some((x, y));
                state.dragging = false;
                // hand the drag to the OS: releases when the button comes up
                let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                SendMessageW(
                    hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(windows::Win32::Foundation::WPARAM(HTCAPTION as usize)),
                    Some(lparam),
                );
                // the send above returns after the mouse-up
                let moved = state
                    .press
                    .map(|(px, py)| {
                        let mut point = windows::Win32::Foundation::POINT { x: 0, y: 0 };
                        let _ = GetCursorPos(&mut point);
                        let client_pt = screen_to_client(hwnd, point);
                        let dx = (client_pt.x - px).pow(2);
                        let dy = (client_pt.y - py).pow(2);
                        (dx + dy).isqrt() as f64
                    })
                    .unwrap_or(f64::INFINITY);
                if moved <= DRAG_THRESHOLD_PX {
                    state.delegate.on_event(PetWindowEvent::Clicked);
                } else {
                    state.dragging = true;
                    report_drag(hwnd, state);
                }
                LRESULT(0)
            }
            WM_RBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    state.delegate.on_event(PetWindowEvent::RightClicked);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                if let Some(state) = window_state(hwnd) {
                    if state.dragging {
                        state.dragging = false;
                        report_drag(hwnd, state);
                    }
                }
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

unsafe fn window_state(
    hwnd: windows::Win32::Foundation::HWND,
) -> Option<&'static mut WindowThreadState> {
    // SAFETY: see `state_receiver`.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{GWLP_USERDATA, GetWindowLongPtrW};
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowThreadState;
        if ptr.is_null() { None } else { Some(&mut *ptr) }
    }
}

unsafe fn screen_to_client(
    hwnd: windows::Win32::Foundation::HWND,
    point: windows::Win32::Foundation::POINT,
) -> windows::Win32::Foundation::POINT {
    // SAFETY: valid HWND + POINT.
    unsafe {
        use windows::Win32::Graphics::Gdi::ScreenToClient;
        let mut point = point;
        let _ = ScreenToClient(hwnd, &mut point);
        point
    }
}

unsafe fn report_drag(hwnd: windows::Win32::Foundation::HWND, state: &WindowThreadState) {
    // SAFETY: valid HWND owned by this thread.
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
        // window position in device pixels
        let mut rect = windows::Win32::Foundation::RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        state.delegate.on_event(PetWindowEvent::Dragged {
            x: rect.left,
            y: rect.top,
        });
    }
}

/// Converts RGBA (straight alpha) to premultiplied BGRA and pushes it to
/// the layered window.
unsafe fn update_layered_frame(
    hwnd: windows::Win32::Foundation::HWND,
    frame: &image::RgbaImage,
) -> Result<(), String> {
    use windows::Win32::Foundation::{POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BLENDFUNCTION, CreateCompatibleDC, CreateDIBSection,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HGDIOBJ, ReleaseDC, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};

    let (width, height) = (frame.width() as i32, frame.height() as i32);
    if width == 0 || height == 0 {
        return Ok(());
    }
    // SAFETY: all handles below are created locally and released before
    // returning; `bits` points at the DIB's own buffer.
    unsafe {
        // top-down DIB: negative height
        let bmi = windows::Win32::Graphics::Gdi::BITMAPINFO {
            bmiHeader: windows::Win32::Graphics::Gdi::BITMAPINFOHEADER {
                biSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>()
                    as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| format!("CreateDIBSection: {e}"))?;
        let old_bitmap = SelectObject(mem_dc, HGDIOBJ(dib.0));

        // premultiply and swap to BGRA
        let dst = bits as *mut u8;
        let src = frame.as_raw();
        let pixel_count = (width * height) as usize;
        for i in 0..pixel_count {
            let r = src[i * 4];
            let g = src[i * 4 + 1];
            let b = src[i * 4 + 2];
            let a = src[i * 4 + 3] as u32;
            *dst.add(i * 4) = ((b as u32 * a + 127) / 255) as u8;
            *dst.add(i * 4 + 1) = ((g as u32 * a + 127) / 255) as u8;
            *dst.add(i * 4 + 2) = ((r as u32 * a + 127) / 255) as u8;
            *dst.add(i * 4 + 3) = a as u8;
        }

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let src_point = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let result = UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            None,
            Some(&size),
            Some(mem_dc),
            Some(&src_point),
            windows::Win32::Foundation::COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        let _ = SelectObject(mem_dc, old_bitmap);
        let _ = DeleteObject(HGDIOBJ(dib.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(Some(hwnd.into()), screen_dc);
        result.map_err(|e| format!("UpdateLayeredWindow failed: {e}"))
    }
}
