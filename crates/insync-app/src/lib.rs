//! InSync desktop application: pet window, tray, hub panels, local stores,
//! alarms and the sync engine wiring.

pub mod app;
pub mod assets;
pub mod clock;
pub mod hub;
pub mod i18n;
pub mod notify;
pub mod palette;
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
