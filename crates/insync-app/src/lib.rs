//! InSync desktop application: pet window, tray, hub panels, local stores,
//! alarms and the sync engine wiring.

pub mod app;
pub mod assets;
pub mod clock;
pub mod hub;
pub mod i18n;
pub mod notify;
pub mod pet;
pub mod store;
pub mod tray;

/// Build environment: Development unless the `production` feature is set.
pub fn build_environment() -> &'static str {
    if cfg!(feature = "production") {
        "production"
    } else {
        "development"
    }
}

/// The InSync data root: `<data_dir>/com.ayangweb.insync/<environment>`.
/// InSync never touches BongoCat's data directories (ADR-0089).
pub fn data_root() -> std::path::PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("com.ayangweb.insync")
        .join(build_environment());
    let _ = bongocat_storage::create_private_dir_all(&base);
    base
}

/// The sprite/asset directory shipped with the app (development builds run
/// from the repo; release builds may override with INSYNC_RESOURCES).
pub fn resources_root() -> std::path::PathBuf {
    if let Ok(custom) = std::env::var("INSYNC_RESOURCES") {
        return std::path::PathBuf::from(custom);
    }
    // repo layout: <repo>/resources/insync — walk up from CWD
    let mut cwd = std::env::current_dir().unwrap_or_default();
    for _ in 0..4 {
        if cwd.join("resources/insync/sprites/manifest.json").exists() {
            return cwd.join("resources/insync");
        }
        cwd = cwd.parent().map(|p| p.to_path_buf()).unwrap_or(cwd.clone());
    }
    cwd.join("resources/insync")
}
