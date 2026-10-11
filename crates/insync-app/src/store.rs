//! InSync's versioned JSON stores: schema-gated, atomic, private-mode, with
//! a corrupt-file quarantine. One store per concern, following the
//! bongocat-config store discipline (see ADR-0089).

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(String),
    #[error("serialize: {0}")]
    Serialize(String),
}

pub trait StoreData: Serialize + DeserializeOwned + Default {
    const FILE_NAME: &'static str;
    const SUBDIR: &'static str;
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// A single JSON file store. Corrupt reads are quarantined and replaced by
/// defaults; writes are atomic private-mode with a timestamped backup of
/// the previous content.
pub struct JsonStore<T: StoreData> {
    path: PathBuf,
    backup_dir: PathBuf,
    _marker: PhantomData<T>,
}

impl<T: StoreData + Send + Sync + 'static> JsonStore<T> {
    pub fn new(root: &Path) -> Self {
        let dir = root.join(T::SUBDIR);
        Self {
            path: dir.join(T::FILE_NAME),
            backup_dir: dir.join("backups"),
            _marker: PhantomData,
        }
    }

    fn ensure_dirs(&self) -> Result<(), StoreError> {
        bongocat_storage::create_private_dir_all(self.path.parent().unwrap_or(Path::new(".")))
            .map_err(|e| StoreError::Io(e.to_string()))?;
        bongocat_storage::create_private_dir_all(&self.backup_dir)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        Ok(())
    }

    /// Loads the stored value; falls back to `T::default()` on any failure
    /// after quarantining the unreadable bytes. A `schema_version` other
    /// than the current one is rejected (never silently migrated).
    pub fn load(&self) -> T {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(_) => return T::default(),
        };
        let accepts = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| match value.get("schema_version") {
                Some(version) => version.as_u64().map(|v| v == SCHEMA_VERSION as u64),
                None => Some(true), // pre-schema files rely on serde defaults
            })
            .unwrap_or(false);
        if !accepts {
            self.quarantine(&bytes);
            return T::default();
        }
        match serde_json::from_slice::<T>(&bytes) {
            Ok(value) => value,
            Err(_) => {
                self.quarantine(&bytes);
                T::default()
            }
        }
    }

    fn quarantine(&self, bytes: &[u8]) {
        let _ = self.ensure_dirs();
        let target = self.backup_dir.join(format!("corrupt-{}.json", now_ms()));
        let _ = std::fs::write(target, bytes);
    }

    pub fn save(&self, value: &T) -> Result<(), StoreError> {
        self.ensure_dirs()?;
        if self.path.exists() {
            let previous = std::fs::read(&self.path).unwrap_or_default();
            let backup = self.backup_dir.join(format!("{}.json", now_ms()));
            let _ = std::fs::write(backup, &previous);
            prune_backups(&self.backup_dir, 3);
        }
        let mut json =
            serde_json::to_vec_pretty(value).map_err(|e| StoreError::Serialize(e.to_string()))?;
        json.push(b'\n');
        bongocat_storage::write_private_atomic(&self.path, &json)
            .map_err(|e| StoreError::Io(e.to_string()))
    }

    /// Deletes the stored file (account removal / cache clear).
    pub fn clear(&self) -> Result<(), StoreError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StoreError::Io(e.to_string())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn prune_backups(dir: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            let modified = entry.metadata().ok()?.modified().ok()?;
            Some((modified, path))
        })
        .collect();
    files.sort();
    while files.len() > keep {
        let (_, path) = files.remove(0);
        let _ = std::fs::remove_file(path);
    }
}

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_server_url")]
    pub server_url: String,
    #[serde(default = "default_pet_scale")]
    pub pet_scale_percent: u16,
    #[serde(default = "default_pet_opacity")]
    pub pet_opacity_percent: u8,
    #[serde(default = "default_true")]
    pub pet_always_on_top: bool,
    #[serde(default)]
    pub pet_click_through: bool,
    #[serde(default = "default_true")]
    pub notifications_enabled: bool,
    #[serde(default = "default_true")]
    pub notification_sound: bool,
    #[serde(default)]
    pub dnd_enabled: bool,
    #[serde(default = "default_true")]
    pub clock_24h: bool,
    #[serde(default = "default_true")]
    pub clock_show_date: bool,
    #[serde(default)]
    pub partner_timezone: Option<String>,
}

fn default_schema() -> u32 {
    SCHEMA_VERSION
}
fn default_language() -> String {
    "zh-CN".to_string()
}
fn default_theme() -> String {
    "system".to_string()
}
fn default_server_url() -> String {
    "http://127.0.0.1:8787".to_string()
}
fn default_pet_scale() -> u16 {
    100
}
fn default_pet_opacity() -> u8 {
    100
}
fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            language: default_language(),
            theme: default_theme(),
            server_url: default_server_url(),
            pet_scale_percent: default_pet_scale(),
            pet_opacity_percent: default_pet_opacity(),
            pet_always_on_top: true,
            pet_click_through: false,
            notifications_enabled: true,
            notification_sound: true,
            dnd_enabled: false,
            clock_24h: true,
            clock_show_date: true,
            partner_timezone: None,
        }
    }
}

impl StoreData for Settings {
    const FILE_NAME: &'static str = "settings.json";
    const SUBDIR: &'static str = "";
}

impl Settings {
    pub fn validate(&mut self) {
        self.pet_scale_percent = self.pet_scale_percent.clamp(50, 400);
        self.pet_opacity_percent = self.pet_opacity_percent.clamp(10, 100);
        if self.language != "zh-CN" && self.language != "en" {
            self.language = "zh-CN".to_string();
        }
        if !self.theme.starts_with("system") && self.theme != "light" && self.theme != "dark" {
            self.theme = "system".to_string();
        }
    }
}

// ---------------------------------------------------------------------------
// account (token lives here; the file is private-mode 0600)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Account {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub saved_at: i64,
}

impl StoreData for Account {
    const FILE_NAME: &'static str = "account.json";
    const SUBDIR: &'static str = "sync";
}

// ---------------------------------------------------------------------------
// window state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PetPlacement {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WindowState {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub pet: Option<PetPlacement>,
    /// The panel section the user last looked at, so reopening the panel comes
    /// back to it. Absent in state files written before the panel had sections.
    #[serde(default)]
    pub hub_tab: Option<String>,
}

impl StoreData for WindowState {
    const FILE_NAME: &'static str = "window-state.json";
    const SUBDIR: &'static str = "";
}

// ---------------------------------------------------------------------------
// personal data (never shared)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalTodo {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
    #[serde(default)]
    pub priority: String, // low|normal|high
    #[serde(default)]
    pub due_at: Option<i64>,
    #[serde(default)]
    pub remind_at: Option<i64>,
    #[serde(default)]
    pub completed_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalNote {
    pub id: String,
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersonalAnniversary {
    pub id: String,
    pub name: String,
    /// `YYYY-MM-DD`
    pub date: String,
    #[serde(default = "default_true")]
    pub repeat_yearly: bool,
    #[serde(default)]
    pub note: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersonalTodos {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub todos: Vec<PersonalTodo>,
}

impl StoreData for PersonalTodos {
    const FILE_NAME: &'static str = "todos.json";
    const SUBDIR: &'static str = "personal";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersonalNotes {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub notes: Vec<PersonalNote>,
}

impl StoreData for PersonalNotes {
    const FILE_NAME: &'static str = "notes.json";
    const SUBDIR: &'static str = "personal";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PersonalAnniversaries {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub anniversaries: Vec<PersonalAnniversary>,
}

impl StoreData for PersonalAnniversaries {
    const FILE_NAME: &'static str = "anniversaries.json";
    const SUBDIR: &'static str = "personal";
}

// ---------------------------------------------------------------------------
// alarms
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmRepeat {
    /// Fires on the stored `once_date` (YYYY-MM-DD).
    Once(String),
    Daily,
    /// 0 = Sunday .. 6 = Saturday.
    Weekdays(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    pub title: String,
    /// Local civil time `HH:MM`.
    pub time: String,
    pub repeat: AlarmRepeat,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub sound: bool,
    /// `YYYY-MM-DD` of the last day this alarm fired.
    #[serde(default)]
    pub last_fired_date: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Alarms {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    #[serde(default)]
    pub alarms: Vec<Alarm>,
}

impl StoreData for Alarms {
    const FILE_NAME: &'static str = "alarms.json";
    const SUBDIR: &'static str = "";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "insync-store-{}-{}-{}",
            std::process::id(),
            now_ms(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_and_recovers_from_corruption() {
        let root = temp_root();
        let store: JsonStore<Settings> = JsonStore::new(&root);
        let settings = Settings {
            pet_scale_percent: 150,
            partner_timezone: Some("Asia/Tokyo".into()),
            ..Settings::default()
        };
        store.save(&settings).unwrap();
        let loaded: Settings = store.load();
        assert_eq!(loaded.pet_scale_percent, 150);
        assert_eq!(loaded.partner_timezone.as_deref(), Some("Asia/Tokyo"));

        // corrupt: quarantine + default
        let path = store.path().to_path_buf();
        std::fs::write(path, b"{not json").unwrap();
        let recovered: Settings = store.load();
        assert_eq!(recovered, Settings::default());
        let backups = std::fs::read_dir(root.join("backups")).unwrap().count();
        assert!(backups >= 1, "corrupt bytes quarantined");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn future_schema_falls_back_to_defaults() {
        let root = temp_root();
        let store: JsonStore<Alarms> = JsonStore::new(&root);
        std::fs::write(store.path(), r#"{"schema_version": 99, "alarms": []}"#).unwrap();
        // strict version gate: non-1 is not silently migrated
        let loaded: Alarms = store.load();
        assert_eq!(loaded, Alarms::default());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_window_state_without_a_panel_section_still_loads() {
        let root = temp_root();
        let store: JsonStore<WindowState> = JsonStore::new(&root);
        // Exactly what an older build wrote: no `hub_tab`, and the pet placed.
        std::fs::write(
            store.path(),
            r#"{"schema_version": 1, "pet": {"x": 640, "y": 480}}"#,
        )
        .unwrap();
        let loaded: WindowState = store.load();
        assert_eq!(loaded.pet, Some(PetPlacement { x: 640, y: 480 }));
        assert_eq!(loaded.hub_tab, None);
        assert_eq!(loaded.schema_version, 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clear_removes_file() {
        let root = temp_root();
        let store: JsonStore<Account> = JsonStore::new(&root);
        store.save(&Account::default()).unwrap();
        assert!(store.path().exists());
        store.clear().unwrap();
        assert!(!store.path().exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
