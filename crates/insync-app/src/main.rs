//! InSync entry point: boots the core services, the pet window, the tray
//! and the hub panel inside one GPUI application.

use gpui_kit::{
    App, Application as GpuiApplication, QuitMode, assets::AllAssets, platform::current_platform,
};
use insync_app::app::{Coordinator, tick_app};
use insync_app::{app::AppCore, assets, hub, pet, tray};
use std::cell::RefCell;

/// Startup and fatal errors go to stderr as single human-readable lines.
#[allow(clippy::print_stderr)]
fn report(line: &str) {
    eprintln!("{line}");
}

fn main() {
    let Ok(core) = AppCore::start() else {
        report("insync-app: failed to start core services");
        std::process::exit(1);
    };
    let core = std::sync::Arc::new(core);

    let icon = load_tray_icon();
    let quit_requested = std::sync::atomic::AtomicBool::new(false);

    GpuiApplication::new_inaccessible(current_platform(false))
        .with_quit_mode(QuitMode::Explicit)
        .with_assets(AllAssets)
        .run(move |cx: &mut App| {
            if !cx.has_global::<gpui_kit::component::theme::Theme>() {
                gpui_kit::init(cx);
            }
            // tray installation needs a live NSApplication; keep it after
            // the GPUI platform is up (same order as BongoCat's tray)
            let tray = tray::install(icon, &core.language()).ok();
            gpui_kit::component::theme::Theme::global_mut(cx)
                .notification
                .placement = gpui_kit::Anchor::BottomRight;

            // pet window + player + behavior live on the main thread
            let pet_window = match pet::PetWindow::create(
                insync_app::pet::initial_device_size(&core),
                insync_app::pet::INITIAL_HEIGHT_PX,
                core.window_store.load().pet.map(|p| (p.x, p.y)),
                core.settings_snapshot().pet_click_through,
                core.settings_snapshot().pet_always_on_top,
            ) {
                Ok(window) => window,
                Err(error) => {
                    report(&format!("insync-app: pet window unavailable: {error}"));
                    return;
                }
            };

            let sprites = assets::sprite_library();
            let character = sprites
                .character("momo")
                .cloned()
                .expect("embedded momo sprites");
            let player = insync_sprite::AnimationPlayer::new(std::sync::Arc::new(character));
            let behavior = insync_sprite::PetBehavior::new(20261011);

            cx.set_global(Coordinator {
                core: std::sync::Arc::clone(&core),
                pet: RefCell::new(pet_window),
                player: RefCell::new(player),
                behavior: RefCell::new(behavior),
                hub: std::sync::Mutex::new(None),
                last_state_revision: std::sync::atomic::AtomicU64::new(0),
                last_notify_version: std::sync::atomic::AtomicU64::new(0),
                pet_visible: std::sync::atomic::AtomicBool::new(true),
                refresh_counter: std::cell::Cell::new(0),
                toggle_hub_requested: std::sync::atomic::AtomicBool::new(false),
            });

            if let Some(tray) = tray {
                cx.spawn(async move |cx| {
                    loop {
                        if let Some(action) = tray.try_action() {
                            match action {
                                tray::TrayAction::OpenHub => {
                                    cx.update(hub::toggle_hub);
                                }
                                tray::TrayAction::PokePartner => {
                                    let now = std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64)
                                        .unwrap_or(0);
                                    cx.update(|cx| {
                                        if let Some(coord) = cx.try_global::<Coordinator>() {
                                            coord.core.engine.command(
                                                insync_client::SyncCommand::Enqueue(
                                                    insync_core::SyncOp::SendPoke {
                                                        client_id: insync_core::new_client_id(),
                                                    },
                                                ),
                                            );
                                            coord.poke_partner(now);
                                        }
                                    });
                                }
                                tray::TrayAction::TogglePetVisible => {
                                    cx.update(|cx| {
                                        if let Some(coord) = cx.try_global::<Coordinator>() {
                                            coord.toggle_pet_visible();
                                        }
                                    });
                                }
                                tray::TrayAction::Quit => {
                                    cx.update(|cx| cx.quit());
                                }
                            }
                        }
                        async_io_timer(std::time::Duration::from_millis(120)).await;
                    }
                })
                .detach();
            }

            // the main loop: pet animation, events, alarms, hub refresh
            cx.spawn(async move |cx| {
                loop {
                    let _ = &quit_requested;
                    cx.update(tick_app);
                    async_io_timer(std::time::Duration::from_millis(80)).await;
                }
            })
            .detach();

            // open the hub once at startup so the user can sign in
            hub::ensure_hub(cx);
        });
}

/// `async_io::Timer` without importing async-io everywhere.
async fn async_io_timer(duration: std::time::Duration) {
    async_io::Timer::after(duration).await;
}

fn load_tray_icon() -> tray_icon::Icon {
    let bytes = assets::tray_icon_bytes();
    match image::load_from_memory(bytes) {
        Ok(decoded) => {
            let rgba = decoded.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());
            tray_icon::Icon::from_rgba(rgba.into_raw(), w, h).unwrap_or_else(|_| fallback_icon())
        }
        Err(_) => fallback_icon(),
    }
}

fn fallback_icon() -> tray_icon::Icon {
    let rgba = vec![0xff; 4 * 16 * 16];
    tray_icon::Icon::from_rgba(rgba, 16, 16).expect("static icon")
}
