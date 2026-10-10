//! System tray + menu. InSync owns its tray integration (muda/tray-icon
//! directly) because the BongoCat platform menu speaks BongoCat actions;
//! see ADR-0089 for the product split.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    OpenHub,
    PokePartner,
    TogglePetVisible,
    Quit,
}

pub struct Tray {
    /// Kept alive: dropping the icon removes it from the menu bar.
    _tray: tray_icon::TrayIcon,
    action_rx: std::sync::mpsc::Receiver<TrayAction>,
}

/// Builds and shows the tray. Returns the action receiver; menu events are
/// drained by the main loop (`try_recv`).
pub fn install(icon: tray_icon::Icon, language: &str) -> Result<Tray, String> {
    let zh = language == "zh-CN";
    let label = |zh_text: &str, en_text: &str| {
        if zh {
            zh_text.to_string()
        } else {
            en_text.to_string()
        }
    };
    let menu = muda::Menu::new();
    let open = muda::MenuItem::with_id("open-hub", label("打开面板", "Open Panel"), true, None);
    let poke = muda::MenuItem::with_id("poke", label("戳戳伴侣", "Poke Partner"), true, None);
    let hide = muda::MenuItem::with_id(
        "toggle-pet",
        label("隐藏/显示桌宠", "Hide/Show Pet"),
        true,
        None,
    );
    let quit = muda::MenuItem::with_id("quit", label("退出", "Quit"), true, None);
    menu.append_items(&[&open, &poke, &hide, &quit]);

    let (tx, rx) = std::sync::mpsc::channel::<TrayAction>();
    // MenuEvent::recv forwards every menu event process-wide; the polling
    // thread translates ids into actions.
    std::thread::Builder::new()
        .name("insync-tray".to_string())
        .spawn(move || {
            let id_to_action = |id: &str| match id {
                "open-hub" => Some(TrayAction::OpenHub),
                "poke" => Some(TrayAction::PokePartner),
                "toggle-pet" => Some(TrayAction::TogglePetVisible),
                "quit" => Some(TrayAction::Quit),
                _ => None,
            };
            let receiver = muda::MenuEvent::receiver();
            while let Ok(event) = receiver.recv() {
                if let Some(action) = id_to_action(&event.id().0) {
                    let _ = tx.send(action);
                }
            }
        });

    let tray = tray_icon::TrayIconBuilder::new()
        .with_id("insync-tray")
        .with_menu(Box::new(menu.clone()))
        .with_menu_on_left_click(true)
        .with_icon(icon)
        .with_tooltip("InSync")
        .build()
        .map_err(|e| e.to_string())?;

    Ok(Tray {
        _tray: tray,
        action_rx: rx,
    })
}

impl Tray {
    pub fn try_action(&self) -> Option<TrayAction> {
        self.action_rx.try_recv().ok()
    }
}
