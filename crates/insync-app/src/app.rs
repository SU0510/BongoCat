//! Application core: stores, engine, audio and the alarm scheduler live in
//! the Send+Sync [`AppCore`]; the main-thread-only pieces (pet window,
//! player, behavior, hub handle) live in the GPUI global [`Coordinator`].

use crate::i18n::text;
use crate::notify::NotificationCenter;
use crate::pet::{PetAppEvent, PetWindow};
use crate::store::*;
use insync_client::{EngineConfig, EngineEvent, EngineHandle, SyncCommand, SyncStatus};
use insync_sprite::{AnimationPlayer, PetBehavior};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Events the main loop receives from background threads.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Alarm {
        id: String,
        title: String,
        sound: bool,
    },
}

pub struct AppCore {
    pub settings: Arc<Mutex<Settings>>,
    pub settings_store: JsonStore<Settings>,
    pub account_store: JsonStore<Account>,
    pub window_store: JsonStore<WindowState>,
    pub todos_store: JsonStore<PersonalTodos>,
    pub notes_store: JsonStore<PersonalNotes>,
    pub anniversaries_store: JsonStore<PersonalAnniversaries>,
    pub alarms_store: JsonStore<Alarms>,
    pub engine: EngineHandle,
    pub notifications: Arc<NotificationCenter>,
    pub alarms_current: Arc<Mutex<Vec<Alarm>>>,
    pub alarm_version: Arc<AtomicU64>,
    pub events_rx: Mutex<std::sync::mpsc::Receiver<AppEvent>>,
    pub status_rx: Mutex<std::sync::mpsc::Receiver<String>>,
    pub status_tx: std::sync::mpsc::Sender<String>,
    pub language: Arc<Mutex<String>>,
}

impl AppCore {
    pub fn start() -> Result<Self, String> {
        let root = crate::data_root();
        let settings_store: JsonStore<Settings> = JsonStore::new(&root);
        let mut settings = settings_store.load();
        settings.validate();
        let _ = settings_store.save(&settings);
        let language = settings.language.clone();
        let settings = Arc::new(Mutex::new(settings));

        let account_store: JsonStore<Account> = JsonStore::new(&root);
        let window_store: JsonStore<WindowState> = JsonStore::new(&root);
        let todos_store: JsonStore<PersonalTodos> = JsonStore::new(&root);
        let notes_store: JsonStore<PersonalNotes> = JsonStore::new(&root);
        let anniversaries_store: JsonStore<PersonalAnniversaries> = JsonStore::new(&root);
        let alarms_store: JsonStore<Alarms> = JsonStore::new(&root);

        // audio: best effort; without a device the app still runs
        let audio = bongocat_audio::MotionAudioService::start(8)
            .map(|service| service.client())
            .unwrap_or_else(|_| bongocat_audio::MotionAudioClient::unavailable());
        // The backend plays only clips it has already decoded, so the chime has
        // to be prepared before the first notification can sound it. Nothing in
        // this app publishes `ActivatePrepared`, so one prepare stays valid.
        if let Some(path) = crate::assets::alarm_sound_path() {
            let _ = audio.try_publish_with_sequence(|sequence| {
                bongocat_audio::MotionAudioCommand::Prepare {
                    sequence,
                    paths: vec![path],
                }
            });
        }
        let notifications = Arc::new(NotificationCenter::new(audio));

        // alarm scheduler thread
        let alarms = alarms_store.load().alarms;
        let alarms_current = Arc::new(Mutex::new(alarms));
        let alarm_version = Arc::new(AtomicU64::new(0));
        let (event_tx, event_rx) = std::sync::mpsc::channel::<AppEvent>();
        {
            let alarms_current = Arc::clone(&alarms_current);
            let version = Arc::clone(&alarm_version);
            let store = JsonStore::<Alarms>::new(&root);
            std::thread::Builder::new()
                .name("insync-alarms".to_string())
                .spawn(move || {
                    let mut seen_version = 0u64;
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        if version.load(Ordering::Relaxed) != seen_version {
                            seen_version = version.load(Ordering::Relaxed);
                            let latest = store.load().alarms;
                            *alarms_current
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) = latest;
                        }
                        let now = jiff::Zoned::now();
                        let mut to_fire = Vec::new();
                        {
                            let mut alarms = alarms_current
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner);
                            for alarm in alarms.iter_mut() {
                                let due = crate::clock::should_fire(alarm, &now)
                                    || crate::clock::missed_recently(alarm, &now);
                                if due {
                                    to_fire.push((
                                        alarm.id.clone(),
                                        alarm.title.clone(),
                                        alarm.sound,
                                    ));
                                    alarm.last_fired_date = Some(crate::clock::date_key(&now));
                                }
                            }
                        }
                        if !to_fire.is_empty() {
                            let snapshot = {
                                let alarms = alarms_current
                                    .lock()
                                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                                Alarms {
                                    schema_version: SCHEMA_VERSION,
                                    alarms: alarms.clone(),
                                }
                            };
                            let _ = store.save(&snapshot);
                            for (id, title, sound) in to_fire {
                                let _ = event_tx.send(AppEvent::Alarm { id, title, sound });
                            }
                        }
                    }
                })
                .map_err(|e| format!("spawn alarm thread: {e}"))?;
        }

        // sync engine
        let server_url = settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .server_url
            .clone();
        let account_store_for_token = JsonStore::<Account>::new(&root);
        let token_source: insync_client::TokenSource = Arc::new(move || {
            let account = account_store_for_token.load();
            if account.token.is_empty() {
                None
            } else {
                Some(account.token)
            }
        });
        let engine = EngineHandle::spawn(EngineConfig {
            server_url,
            token_source,
        });
        if !account_store.load().token.is_empty() {
            engine.command(SyncCommand::Connect);
        }

        let (status_tx, status_rx) = std::sync::mpsc::channel::<String>();
        Ok(Self {
            settings,
            settings_store,
            account_store,
            window_store,
            todos_store,
            notes_store,
            anniversaries_store,
            alarms_store,
            engine,
            notifications,
            alarms_current,
            alarm_version,
            events_rx: Mutex::new(event_rx),
            status_rx: Mutex::new(status_rx),
            status_tx,
            language: Arc::new(Mutex::new(language)),
        })
    }

    pub fn save_settings(&self, settings: Settings) {
        let mut settings = settings;
        settings.validate();
        let _ = self.settings_store.save(&settings);
        *self
            .settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = settings;
        self.alarm_version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn settings_snapshot(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn update_alarms(&self, alarms: Vec<Alarm>) {
        let snapshot = Alarms {
            schema_version: SCHEMA_VERSION,
            alarms: alarms.clone(),
        };
        let _ = self.alarms_store.save(&snapshot);
        *self
            .alarms_current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = alarms;
        self.alarm_version.fetch_add(1, Ordering::Relaxed);
    }

    pub fn signed_in(&self) -> bool {
        !self.account_store.load().token.is_empty()
    }

    pub fn status_text(&self) -> String {
        let locale = if self.language() == "zh-CN" {
            "zh-CN"
        } else {
            "en"
        };
        match self.engine.state.status() {
            SyncStatus::Online => text(locale, "status.online").to_string(),
            SyncStatus::Connecting | SyncStatus::Retrying(_) => {
                text(locale, "status.retrying").to_string()
            }
            SyncStatus::NeedsRelogin => text(locale, "status.relogin").to_string(),
            SyncStatus::Offline => text(locale, "status.offline").to_string(),
        }
    }

    /// Saves the pet placement after a drag.
    pub fn language(&self) -> String {
        self.language
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn save_pet_position(&self, x: i32, y: i32) {
        let mut state = self.window_store.load();
        state.pet = Some(PetPlacement { x, y });
        let _ = self.window_store.save(&state);
    }

    /// Remembers which panel section the user was on, so the panel opens there
    /// next time instead of always jumping back to the first tab.
    pub fn save_hub_tab(&self, key: &str) {
        let mut state = self.window_store.load();
        state.hub_tab = Some(key.to_string());
        let _ = self.window_store.save(&state);
    }
}

// ---------------------------------------------------------------------------
// main-thread coordinator (GPUI global; not Send)
// ---------------------------------------------------------------------------

/// Main-thread-only state. GPUI globals live on the main thread, so the
/// non-Send pet window handle and player are safe behind RefCells here.
pub struct Coordinator {
    pub core: Arc<AppCore>,
    pub pet: RefCell<PetWindow>,
    pub player: RefCell<AnimationPlayer>,
    pub behavior: RefCell<PetBehavior>,
    pub hub: Mutex<Option<crate::hub::HubWindowHandle>>,
    pub last_state_revision: AtomicU64,
    pub last_notify_version: AtomicU64,
    pub pet_visible: AtomicBool,
    pub refresh_counter: std::cell::Cell<u8>,
    pub toggle_hub_requested: AtomicBool,
}

impl gpui_kit::Global for Coordinator {}

impl Coordinator {
    pub fn request_toggle_hub(&self) {
        self.toggle_hub_requested.store(true, Ordering::Relaxed);
    }

    pub fn take_toggle_hub_request(&self) -> bool {
        self.toggle_hub_requested.swap(false, Ordering::Relaxed)
    }

    pub fn pet_visible(&self) -> bool {
        self.pet_visible.load(Ordering::Relaxed)
    }

    pub fn toggle_pet_visible(&self) {
        let next = !self.pet_visible();
        self.pet_visible.store(next, Ordering::Relaxed);
        self.pet.borrow().handle.set_visible(next);
    }

    /// Local-first poke: the pet waves immediately; the send is queued
    /// separately by the caller.
    pub fn poke_partner(&self, now_ms: u64) {
        let _ = self.behavior.borrow_mut().handle(
            insync_sprite::PetEvent::PokeSent,
            &mut self.player.borrow_mut(),
            now_ms,
        );
    }

    fn handle_pet_event(&self, event: PetAppEvent) {
        let now_ms = now_ms();
        match event {
            PetAppEvent::Clicked => {
                let _ = self.behavior.borrow_mut().handle(
                    insync_sprite::PetEvent::Click,
                    &mut self.player.borrow_mut(),
                    now_ms as u64,
                );
            }
            PetAppEvent::RightClicked => self.request_toggle_hub(),
            PetAppEvent::Dragged => {
                if let Some((x, y)) = self.pet.borrow().handle.position() {
                    self.core.save_pet_position(x, y);
                }
            }
        }
    }

    fn handle_engine_event(&self, event: &EngineEvent) {
        let now_ms = now_ms();
        let settings = self.core.settings_snapshot();
        let locale = if self.core.language() == "zh-CN" {
            "zh-CN"
        } else {
            "en"
        };
        match event {
            EngineEvent::Event(frame) => match frame.kind {
                insync_core::EventKind::PokeReceived => {
                    self.core.notifications.offer(
                        "poke",
                        text(locale, "notify.poked").into(),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::Poked,
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                insync_core::EventKind::MessageCreated => {
                    self.core.notifications.offer(
                        "msg",
                        text(locale, "notify.new_message").into(),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::MessageArrived,
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                insync_core::EventKind::LetterCreated => {
                    self.core.notifications.offer(
                        "letter",
                        text(locale, "notify.new_letter").into(),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::LetterArrived,
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                insync_core::EventKind::FocusInvited => {
                    self.core.notifications.offer(
                        "focus-invite",
                        text(locale, "notify.focus_invite").into(),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::FocusInviteArrived,
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                insync_core::EventKind::FocusStarted => {
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::FocusStarted,
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                insync_core::EventKind::FocusCompleted => {
                    self.core.notifications.offer(
                        "focus-done",
                        text(locale, "notify.focus_done").into(),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::FocusEnded { celebrate: true },
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                insync_core::EventKind::RelationDissolved => {
                    let _ = self.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::FocusEnded { celebrate: false },
                        &mut self.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
                _ => {}
            },
            EngineEvent::StatusChanged(status) => {
                if *status == SyncStatus::NeedsRelogin {
                    self.core.notifications.offer(
                        "relogin",
                        text(locale, "status.relogin").into(),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                }
            }
            EngineEvent::NoteConflict { .. } => {
                self.core.notifications.offer(
                    "note-conflict",
                    if locale == "zh-CN" {
                        "便签有冲突，已载入最新内容".into()
                    } else {
                        "Note conflict: latest loaded".into()
                    },
                    String::new(),
                    &settings,
                    now_ms,
                );
            }
            EngineEvent::PokeFailed | EngineEvent::OpFailed { .. } => {}
            EngineEvent::OverviewRefreshed => {}
        }
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// One iteration of the main loop. Runs on the GPUI main thread.
pub fn tick_app(cx: &mut gpui_kit::App) {
    if cx.try_global::<Coordinator>().is_none() {
        return;
    }
    if cx
        .try_global::<Coordinator>()
        .is_some_and(|coordinator| coordinator.take_toggle_hub_request())
    {
        crate::hub::toggle_hub(cx);
    }
    let now_ms = now_ms();

    // 1. pet window events
    if let Some(coordinator) = cx.try_global::<Coordinator>() {
        loop {
            let next = coordinator.pet.borrow().events.try_recv();
            match next {
                Ok(event) => coordinator.handle_pet_event(event),
                Err(_) => break,
            }
        }
    }

    // 2. alarm + background-op events
    if let Some(coordinator) = cx.try_global::<Coordinator>() {
        let receiver = coordinator
            .core
            .events_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while let Ok(event) = receiver.try_recv() {
            match event {
                AppEvent::Alarm { title, sound, .. } => {
                    let settings = coordinator.core.settings_snapshot();
                    coordinator.core.notifications.offer(
                        "alarm",
                        format!("🔔 {}", title),
                        String::new(),
                        &settings,
                        now_ms,
                    );
                    let _ = sound;
                    let _ = coordinator.behavior.borrow_mut().handle(
                        insync_sprite::PetEvent::Click,
                        &mut coordinator.player.borrow_mut(),
                        now_ms as u64,
                    );
                }
            }
        }
        let receiver = coordinator
            .core
            .status_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while let Ok(message) = receiver.try_recv() {
            let settings = coordinator.core.settings_snapshot();
            coordinator.core.notifications.offer(
                "status",
                message,
                String::new(),
                &settings,
                now_ms,
            );
        }
    }

    // 3. engine events
    if let Some(coordinator) = cx.try_global::<Coordinator>() {
        while let Some(event) = coordinator.core.engine.try_recv_event() {
            coordinator.handle_engine_event(&event);
        }
    }

    // 4. animation: random life actions + one-shot completion
    if let Some(coordinator) = cx.try_global::<Coordinator>() {
        let _ = coordinator
            .behavior
            .borrow_mut()
            .update(&mut coordinator.player.borrow_mut(), now_ms as u64);
        {
            let player = coordinator.player.borrow();
            if player.finished(now_ms as u64)
                && player.active_name() != Some(player.base_animation())
            {
                drop(player);
                coordinator.player.borrow_mut().play_base(now_ms as u64);
            }
        }

        // 5. render the current frame (scaled to the configured size)
        let frame = coordinator.player.borrow().current_frame(now_ms as u64);
        if let Some(frame) = frame {
            let scale_percent = coordinator.core.settings_snapshot().pet_scale_percent;
            let target_w = (frame.width() * scale_percent as u32 / 100).max(32);
            let target_h = (frame.height() * scale_percent as u32 / 100).max(32);
            let frame = if (frame.width(), frame.height()) != (target_w, target_h) {
                image::imageops::resize(
                    &frame,
                    target_w,
                    target_h,
                    image::imageops::FilterType::Nearest,
                )
            } else {
                frame
            };
            coordinator.pet.borrow().handle.set_frame(frame);
        }
        coordinator.pet.borrow().handle.apply_pending();
    }

    // 6. refresh the hub when state moved or every ~0.5s for countdowns
    let needs_refresh = cx
        .try_global::<Coordinator>()
        .map(|coordinator| {
            let state_revision = coordinator.core.engine.state.revision();
            let notify_version = coordinator.core.notifications.version();
            let state_moved = state_revision
                != coordinator.last_state_revision.load(Ordering::Relaxed)
                || notify_version != coordinator.last_notify_version.load(Ordering::Relaxed);
            let counter = coordinator.refresh_counter.get().wrapping_add(1);
            coordinator.refresh_counter.set(counter);
            if state_moved || counter >= 6 {
                coordinator
                    .last_state_revision
                    .store(state_revision, Ordering::Relaxed);
                coordinator
                    .last_notify_version
                    .store(notify_version, Ordering::Relaxed);
                coordinator.refresh_counter.set(0);
                true
            } else {
                false
            }
        })
        .unwrap_or(false);
    if needs_refresh {
        cx.refresh_windows();
    }
}
