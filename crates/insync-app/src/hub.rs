//! The hub panel: one GPUI window with tabs for chat, letters, focus,
//! todos, notes, anniversaries, alarms and settings. Independent of the
//! pet window; opening/closing it never touches the animation.

use crate::app::Coordinator;
use crate::clock;
use crate::i18n::text;
use crate::palette;
use crate::store::{Alarm, AlarmRepeat, PersonalAnniversary, PersonalNote, PersonalTodo};
use gpui_kit::assets::IconName;
use gpui_kit::base::v_flex;
use gpui_kit::component::avatar::Avatar;
use gpui_kit::component::bubble::{Bubble, BubbleVariant};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::empty::{
    Empty, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::message::MessageAlignment;
use gpui_kit::component::progress::ProgressCircle;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Icon, Root, Sizable, Size};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, Subscription, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};
use gpui_kit::{FontWeight, SharedString};
use insync_client::SyncCommand;
use insync_core::{FocusState, PresenceState, SyncOp};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// The mascot portrait the panel's header and empty states show. Cut from the
/// character sheet by `tools/insync-ui/generate_ui_images.py` and served by
/// `crate::assets::InSyncAssets`.
const PORTRAIT: &str = "insync/momo_portrait.png";

/// The waving pose, used where an empty panel needs a friendlier face than a
/// 48px avatar.
const WAVE: &str = "insync/momo_wave.png";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HubTab {
    Chat,
    Letters,
    Focus,
    Todos,
    Notes,
    Anniversaries,
    Alarms,
    Settings,
}

const TABS: [HubTab; 8] = [
    HubTab::Chat,
    HubTab::Letters,
    HubTab::Focus,
    HubTab::Todos,
    HubTab::Notes,
    HubTab::Anniversaries,
    HubTab::Alarms,
    HubTab::Settings,
];

impl HubTab {
    fn index(self) -> usize {
        TABS.iter().position(|t| *t == self).unwrap_or(0)
    }

    fn title(self, language: &str) -> &'static str {
        match self {
            HubTab::Chat => text(language, "tab.chat"),
            HubTab::Letters => text(language, "tab.letters"),
            HubTab::Focus => text(language, "tab.focus"),
            HubTab::Todos => text(language, "tab.todos"),
            HubTab::Notes => text(language, "tab.notes"),
            HubTab::Anniversaries => text(language, "tab.anniversaries"),
            HubTab::Alarms => text(language, "tab.alarms"),
            HubTab::Settings => text(language, "tab.settings"),
        }
    }

    /// The tab's tile glyph. Eight tabs of two-character labels are hard to
    /// tell apart at a glance; the icon is what makes the tile recognisable
    /// before the label is read.
    fn icon(self) -> IconName {
        match self {
            HubTab::Chat => IconName::MessageSquareHeart,
            HubTab::Letters => IconName::MailOpen,
            HubTab::Focus => IconName::Timer,
            HubTab::Todos => IconName::ListTodo,
            HubTab::Notes => IconName::StickyNote,
            HubTab::Anniversaries => IconName::CalendarHeart,
            HubTab::Alarms => IconName::AlarmClock,
            HubTab::Settings => IconName::Settings,
        }
    }

    /// The name this section is stored under, matching the `tab.<key>` label.
    fn key(self) -> &'static str {
        match self {
            HubTab::Chat => "chat",
            HubTab::Letters => "letters",
            HubTab::Focus => "focus",
            HubTab::Todos => "todos",
            HubTab::Notes => "notes",
            HubTab::Anniversaries => "anniversaries",
            HubTab::Alarms => "alarms",
            HubTab::Settings => "settings",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        TABS.into_iter().find(|tab| tab.key() == key)
    }
}

pub struct HubView {
    core: Arc<crate::app::AppCore>,
    tab: HubTab,
    chat_input: Entity<InputState>,
    letter_input: Entity<InputState>,
    todo_input: Entity<InputState>,
    note_input: Entity<InputState>,
    anniversary_name_input: Entity<InputState>,
    anniversary_date_input: Entity<InputState>,
    alarm_title_input: Entity<InputState>,
    alarm_time_input: Entity<InputState>,
    server_input: Entity<InputState>,
    username_input: Entity<InputState>,
    password_input: Entity<InputState>,
    invite_input: Entity<InputState>,
    focus_topic_input: Entity<InputState>,
    personal_note_selected: Option<String>,
    shared_note_selected: Option<String>,
    focus_duration: i64,
    _appearance: Subscription,
}

pub struct HubWindowHandle {
    pub window: WindowHandle<Root>,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Wall-clock label for a stored timestamp, in the machine's own zone.
///
/// Empty when the timestamp cannot be read back, so a bad record leaves a
/// blank line rather than a wrong hour.
fn clock_time(timestamp: insync_core::UnixMs) -> String {
    jiff::Timestamp::from_millisecond(timestamp)
        .map(|moment| moment.to_zoned(jiff::tz::TimeZone::system()))
        .map(|zoned| clock::format_time(&zoned, true))
        .unwrap_or_default()
}

/// Opens the hub window and stores the handle in the coordinator.
fn open_hub(cx: &mut App) -> Option<HubWindowHandle> {
    let coordinator = cx.try_global::<Coordinator>()?;
    let core = Arc::clone(&coordinator.core);
    let language = core.language();
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds {
            origin: Default::default(),
            size: size(px(400.), px(620.)),
        })),
        titlebar: Some(TitlebarOptions {
            title: Some(text(&language, "app.name").into()),
            ..Default::default()
        }),
        window_min_size: Some(size(px(360.), px(500.))),
        ..Default::default()
    };
    let view_slot: Rc<RefCell<Option<Entity<HubView>>>> = Rc::new(RefCell::new(None));
    let view_slot_for_builder = Rc::clone(&view_slot);
    let opened = cx.open_window(options, move |window, cx| {
        if !cx.has_global::<gpui_kit::component::Theme>() {
            gpui_kit::init(cx);
        }
        // The panel wears the product palette, and keeps wearing it: a system
        // appearance change reloads the base theme, so the palette is written
        // again on every switch.
        palette::install_for(palette::for_appearance(window.appearance()), cx);
        let view = cx.new(|cx| HubView::new(&core, window, cx));
        *view_slot_for_builder.borrow_mut() = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    });
    let window = opened.ok()?;
    view_slot
        .borrow()
        .is_some()
        .then_some(HubWindowHandle { window })
}

fn store_hub(cx: &App, handle: HubWindowHandle) {
    if let Some(coordinator) = cx.try_global::<Coordinator>() {
        *coordinator
            .hub
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
    }
}

/// Takes the stored handle if its window is still alive, together with whether
/// that window is the active one.
///
/// A window the user closed with its own close button leaves the stored handle
/// pointing at a window that no longer exists; `is_active` answers `None` for
/// it, and dropping the handle here is what lets the next "Open Panel" build a
/// fresh window instead of acting on a dead one.
fn take_live_hub(cx: &mut App) -> Option<(HubWindowHandle, bool)> {
    let handle = cx.try_global::<Coordinator>().and_then(|coordinator| {
        coordinator
            .hub
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    })?;
    let active = handle.window.is_active(cx)?;
    Some((handle, active))
}

/// Brings the panel to the front, using the window we already have.
///
/// Ordering the window front is not enough on its own: InSync is an accessory
/// (menu bar) app, so it is never the active app unless it says so, and an
/// inactive app's window is placed behind every other app's window. Creating
/// the panel without this left it buried underneath whatever the user was
/// working in — with the pet panel, which floats above normal windows, still
/// visible on top — which is why "Open Panel" looked like it did nothing.
fn raise_hub(cx: &mut App, handle: HubWindowHandle) {
    cx.activate(true);
    let _ = handle
        .window
        .update(cx, |_, window, _| window.activate_window());
    store_hub(cx, handle);
}

/// Shows the panel: opens the window when it does not exist, otherwise raises
/// the existing one.
pub fn show_hub(cx: &mut App) {
    apply_gesture(cx, HubGesture::Show);
}

/// Pet context click: closes a panel that is already in front, and raises one
/// that is not (a closed or hidden-away panel must never swallow the gesture).
pub fn toggle_hub(cx: &mut App) {
    apply_gesture(cx, HubGesture::Toggle);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HubGesture {
    /// The "Open Panel" menu item: show the panel, never close it.
    Show,
    /// The pet's context click, which may close a panel that is already in front.
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HubIntent {
    Open,
    Close,
}

/// Resolves a gesture and what is known about the window into what to do.
///
/// `window_state` is `Some(active)` for a live window and `None` when there is
/// none (or the stored handle points at a window that no longer exists).
///
/// The two entry points differ in exactly one case: a panel that is already the
/// active window. "Open Panel" still opens — closing the panel that the app had
/// opened at startup is what made the menu item look like it did nothing —
/// while the pet's click closes it.
fn intent(gesture: HubGesture, window_state: Option<bool>) -> HubIntent {
    match (gesture, window_state) {
        (HubGesture::Show, _) => HubIntent::Open,
        (HubGesture::Toggle, Some(true)) => HubIntent::Close,
        (HubGesture::Toggle, Some(false) | None) => HubIntent::Open,
    }
}

fn apply_gesture(cx: &mut App, gesture: HubGesture) {
    let live = take_live_hub(cx);
    match intent(gesture, live.as_ref().map(|(_, active)| *active)) {
        HubIntent::Open => match live {
            Some((handle, _)) => raise_hub(cx, handle),
            None => {
                if let Some(handle) = open_hub(cx) {
                    raise_hub(cx, handle);
                }
            }
        },
        HubIntent::Close => {
            if let Some((handle, _)) = live {
                let closed = handle
                    .window
                    .update(cx, |_, window, _| window.remove_window())
                    .is_ok();
                if !closed {
                    raise_hub(cx, handle);
                }
            }
        }
    }
}

impl HubView {
    fn new(core: &Arc<crate::app::AppCore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let language = core.language();
        let input =
            |window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx));
        let input_with = |key: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(text(&language, key)))
        };
        // Every window carries its own appearance, so the switch is observed
        // here rather than app-wide: the palette is re-written for whatever the
        // window has just become.
        let appearance = window.observe_window_appearance(|window, cx| {
            palette::install_for(palette::for_appearance(window.appearance()), cx);
        });
        Self {
            core: Arc::clone(core),
            tab: core
                .window_store
                .load()
                .hub_tab
                .as_deref()
                .and_then(HubTab::from_key)
                .unwrap_or(HubTab::Chat),
            chat_input: input_with("chat.placeholder", window, cx),
            letter_input: input_with("letters.placeholder", window, cx),
            todo_input: input_with("todos.add", window, cx),
            note_input: input(window, cx),
            anniversary_name_input: input_with("days.add", window, cx),
            anniversary_date_input: input_with("days.add", window, cx),
            alarm_title_input: input_with("alarms.add", window, cx),
            alarm_time_input: input_with("alarms.time", window, cx),
            server_input: input_with("settings.server", window, cx),
            username_input: input_with("settings.username", window, cx),
            password_input: input_with("settings.password", window, cx),
            invite_input: input_with("settings.invite_code", window, cx),
            focus_topic_input: input_with("focus.topic", window, cx),
            personal_note_selected: None,
            shared_note_selected: None,
            focus_duration: 1500,
            _appearance: appearance,
        }
    }

    fn language(&self) -> String {
        self.core.language()
    }

    fn t(&self, key: &str) -> &'static str {
        text(&self.language(), key)
    }

    // -------------------------------------------------------------- actions

    fn send_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.chat_input.read(cx).value().to_string();
        if body.trim().is_empty() {
            return;
        }
        let seq = self
            .core
            .engine
            .state
            .read(|s| s.messages.last().map(|m| m.seq))
            .flatten()
            .unwrap_or(0);
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::SendMessage {
                client_id: insync_core::new_client_id(),
                body,
            }));
        if seq > 0 {
            self.core
                .engine
                .command(SyncCommand::Enqueue(SyncOp::MarkRead {
                    last_message_seq: seq,
                }));
        }
        self.chat_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn send_letter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.letter_input.read(cx).value().to_string();
        if body.trim().is_empty() {
            return;
        }
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::SendLetter {
                client_id: insync_core::new_client_id(),
                body,
            }));
        self.letter_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn invite_focus(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let topic = self.focus_topic_input.read(cx).value().to_string();
        let topic = if topic.trim().is_empty() {
            None
        } else {
            Some(topic)
        };
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::FocusInvite {
                client_id: insync_core::new_client_id(),
                topic,
                duration_secs: self.focus_duration,
            }));
        cx.notify();
    }

    fn focus_action(
        &mut self,
        _window: &mut Window,
        action: insync_core::FocusAction,
        cx: &mut Context<Self>,
    ) {
        if let Some(focus) = self
            .core
            .engine
            .state
            .read(|s| s.active_focus.clone())
            .flatten()
        {
            self.core
                .engine
                .command(SyncCommand::Enqueue(SyncOp::FocusAction {
                    focus_id: focus.id,
                    action,
                    base_revision: focus.revision,
                }));
        }
        cx.notify();
    }

    fn add_personal_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.todo_input.read(cx).value().to_string();
        if title.trim().is_empty() {
            return;
        }
        let now = now_ms();
        let mut data = self.core.todos_store.load();
        data.todos.insert(
            0,
            PersonalTodo {
                id: insync_core::new_client_id(),
                title,
                detail: None,
                priority: "normal".into(),
                due_at: None,
                remind_at: None,
                completed_at: None,
                created_at: now,
                updated_at: now,
            },
        );
        let _ = self.core.todos_store.save(&data);
        self.todo_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn toggle_personal_todo(&mut self, _window: &mut Window, id: &str, cx: &mut Context<Self>) {
        let now = now_ms();
        let mut data = self.core.todos_store.load();
        if let Some(todo) = data.todos.iter_mut().find(|t| t.id == id) {
            todo.completed_at = if todo.completed_at.is_some() {
                None
            } else {
                Some(now)
            };
            todo.updated_at = now;
        }
        let _ = self.core.todos_store.save(&data);
        cx.notify();
    }

    fn delete_personal_todo(&mut self, _window: &mut Window, id: &str, cx: &mut Context<Self>) {
        let mut data = self.core.todos_store.load();
        data.todos.retain(|t| t.id != id);
        let _ = self.core.todos_store.save(&data);
        cx.notify();
    }

    fn add_shared_todo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.todo_input.read(cx).value().to_string();
        if title.trim().is_empty() {
            return;
        }
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::CreateTodo {
                client_id: insync_core::new_client_id(),
                title,
                detail: None,
                priority: insync_core::Priority::Normal,
                due_at: None,
            }));
        self.todo_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn toggle_shared_todo(
        &mut self,
        _window: &mut Window,
        id: String,
        completed: bool,
        cx: &mut Context<Self>,
    ) {
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::UpdateTodo {
                todo_id: id,
                req: insync_core::api::UpdateTodoRequest {
                    title: None,
                    detail: None,
                    priority: None,
                    due_at: None,
                    completed: Some(!completed),
                },
            }));
        cx.notify();
    }

    fn add_personal_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let now = now_ms();
        let mut data = self.core.notes_store.load();
        let note = PersonalNote {
            id: insync_core::new_client_id(),
            content: String::new(),
            created_at: now,
            updated_at: now,
        };
        self.personal_note_selected = Some(note.id.clone());
        data.notes.insert(0, note);
        let _ = self.core.notes_store.save(&data);
        cx.notify();
    }

    fn save_personal_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.personal_note_selected.clone() else {
            return;
        };
        let content = self.note_input.read(cx).value().to_string();
        let mut data = self.core.notes_store.load();
        if let Some(note) = data.notes.iter_mut().find(|n| n.id == id) {
            note.content = content;
            note.updated_at = now_ms();
        }
        let _ = self.core.notes_store.save(&data);
        cx.notify();
    }

    fn save_shared_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.shared_note_selected.clone() else {
            return;
        };
        let content = self.note_input.read(cx).value().to_string();
        let base_revision = self
            .core
            .engine
            .state
            .read(|s| {
                s.shared_notes
                    .iter()
                    .find(|n| n.id == id)
                    .map(|n| n.revision)
            })
            .flatten()
            .unwrap_or(0);
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::UpdateNote {
                note_id: id,
                content,
                base_revision,
            }));
        cx.notify();
    }

    fn add_shared_note(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let content = self.note_input.read(cx).value().to_string();
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::CreateNote {
                client_id: insync_core::new_client_id(),
                content,
            }));
        cx.notify();
    }

    fn add_anniversary(&mut self, window: &mut Window, shared: bool, cx: &mut Context<Self>) {
        let name = self.anniversary_name_input.read(cx).value().to_string();
        let date = self.anniversary_date_input.read(cx).value().to_string();
        if name.trim().is_empty() {
            return;
        }
        if shared {
            self.core
                .engine
                .command(SyncCommand::Enqueue(SyncOp::CreateAnniversary {
                    client_id: insync_core::new_client_id(),
                    req: insync_core::api::CreateAnniversaryRequest {
                        client_id: insync_core::new_client_id(),
                        name,
                        date,
                        repeat_yearly: true,
                        note: None,
                    },
                }));
        } else {
            let mut data = self.core.anniversaries_store.load();
            data.anniversaries.push(PersonalAnniversary {
                id: insync_core::new_client_id(),
                name,
                date,
                repeat_yearly: true,
                note: None,
                created_at: now_ms(),
            });
            let _ = self.core.anniversaries_store.save(&data);
        }
        self.anniversary_name_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.anniversary_date_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn add_alarm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.alarm_title_input.read(cx).value().to_string();
        let time = self.alarm_time_input.read(cx).value().to_string();
        if clock::parse_hhmm(&time).is_none() {
            return;
        }
        let mut alarms = self.core.alarms_store.load();
        alarms.alarms.push(Alarm {
            id: insync_core::new_client_id(),
            title: if title.trim().is_empty() {
                time.clone()
            } else {
                title
            },
            time,
            repeat: AlarmRepeat::Daily,
            enabled: true,
            sound: true,
            last_fired_date: None,
            created_at: now_ms(),
        });
        self.core.update_alarms(alarms.alarms);
        self.alarm_title_input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn toggle_alarm(&mut self, _window: &mut Window, id: &str, cx: &mut Context<Self>) {
        let mut alarms = self.core.alarms_store.load();
        if let Some(alarm) = alarms.alarms.iter_mut().find(|a| a.id == id) {
            alarm.enabled = !alarm.enabled;
        }
        self.core.update_alarms(alarms.alarms);
        cx.notify();
    }

    fn delete_alarm(&mut self, _window: &mut Window, id: &str, cx: &mut Context<Self>) {
        let mut alarms = self.core.alarms_store.load();
        alarms.alarms.retain(|a| a.id != id);
        self.core.update_alarms(alarms.alarms);
        cx.notify();
    }

    fn set_poke(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.core
            .engine
            .command(SyncCommand::Enqueue(SyncOp::SendPoke {
                client_id: insync_core::new_client_id(),
            }));
        cx.notify();
    }

    fn set_language(&mut self, language: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut settings = self.core.settings_snapshot();
        settings.language = language.to_string();
        self.core.save_settings(settings);
        *self
            .core
            .language
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = language.to_string();
        self.relabel_inputs(window, cx);
        cx.notify();
    }

    /// Re-labels the input placeholders after a language switch.
    ///
    /// The placeholders are state on the input, not text drawn from the view's
    /// current language, so they have to be written again when that language
    /// changes — otherwise the hints stay in the language the panel opened in.
    fn relabel_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let language = self.language();
        let chat = self.chat_input.clone();
        let letter = self.letter_input.clone();
        let todo = self.todo_input.clone();
        let anniversary_name = self.anniversary_name_input.clone();
        let anniversary_date = self.anniversary_date_input.clone();
        let alarm_title = self.alarm_title_input.clone();
        let alarm_time = self.alarm_time_input.clone();
        let server = self.server_input.clone();
        let username = self.username_input.clone();
        let password = self.password_input.clone();
        let invite = self.invite_input.clone();
        let focus_topic = self.focus_topic_input.clone();
        for (input, key) in [
            (chat, "chat.placeholder"),
            (letter, "letters.placeholder"),
            (todo, "todos.add"),
            (anniversary_name, "days.add"),
            (anniversary_date, "days.add"),
            (alarm_title, "alarms.add"),
            (alarm_time, "alarms.time"),
            (server, "settings.server"),
            (username, "settings.username"),
            (password, "settings.password"),
            (invite, "settings.invite_code"),
            (focus_topic, "focus.topic"),
        ] {
            let placeholder = text(&language, key);
            input.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx);
            });
        }
    }

    /// Runs a REST account operation on a background thread; results are
    /// reported through the status channel and shown as notifications.
    fn spawn_account_op(&mut self, op: AccountOp, cx: &mut Context<Self>) {
        let core = Arc::clone(&self.core);
        let server = core.settings_snapshot().server_url;
        let username = self.username_input.read(cx).value().to_string();
        let password = self.password_input.read(cx).value().to_string();
        let invite = self.invite_input.read(cx).value().to_string();
        let status_tx = core.status_tx.clone();
        std::thread::spawn(move || {
            let report = |message: String| {
                let _ = status_tx.send(message);
            };
            let token = core.account_store.load().token;
            let mut client = insync_client::InSyncClient::new(&server);
            match op {
                AccountOp::Register => {
                    match insync_client::blocking::run(client.register(
                        &insync_core::api::RegisterRequest {
                            username: username.clone(),
                            password: password.clone(),
                            display_name: username,
                        },
                    )) {
                        Ok(user) => {
                            let token = client.token().unwrap_or_default().to_string();
                            save_account(
                                &core,
                                &user.id,
                                &user.username,
                                &user.display_name,
                                &token,
                            );
                            report("注册成功 ✓".into());
                        }
                        Err(error) => report(format!("注册失败: {error}")),
                    }
                }
                AccountOp::Login => {
                    match insync_client::blocking::run(client.login(
                        &insync_core::api::AuthRequest {
                            username: username.clone(),
                            password: password.clone(),
                        },
                    )) {
                        Ok(user) => {
                            let token = client.token().unwrap_or_default().to_string();
                            save_account(
                                &core,
                                &user.id,
                                &user.username,
                                &user.display_name,
                                &token,
                            );
                            report("登录成功 ✓".into());
                        }
                        Err(error) => report(format!("登录失败: {error}")),
                    }
                }
                AccountOp::Invite => {
                    match insync_client::blocking::run(client.with_token(&token).create_invite()) {
                        Ok(invite) => report(format!("邀请码: {} (24 小时有效)", invite.code)),
                        Err(error) => report(format!("创建邀请失败: {error}")),
                    }
                }
                AccountOp::AcceptInvite => {
                    match insync_client::blocking::run(
                        client.with_token(&token).accept_invite(&invite),
                    ) {
                        Ok(pair) => report(format!("绑定成功: {}", pair.partner.display_name)),
                        Err(error) => report(format!("绑定失败: {error}")),
                    }
                }
                AccountOp::Dissolve => {
                    match insync_client::blocking::run(client.with_token(&token).dissolve()) {
                        Ok(()) => report("已解除绑定".into()),
                        Err(error) => report(format!("解除绑定失败: {error}")),
                    }
                }
                AccountOp::Logout => {
                    let _ = insync_client::blocking::run(client.with_token(&token).logout());
                    let _ = JsonStoreAccount(&core).clear();
                    report("已退出登录".into());
                }
            }
        });
        cx.notify();
    }

    // -------------------------------------------------------------- render

    /// A group of related rows on its own surface, separated from the canvas
    /// by a hairline border rather than a shadow.
    fn card(&self, cx: &Context<Self>) -> gpui_kit::Div {
        let theme = cx.theme();
        div()
            .w_full()
            .p(theme.spacing_tokens().sm)
            .rounded(theme.radius_lg)
            .bg(theme.colors.popover)
            .border_1()
            .border_color(theme.colors.border)
    }

    /// A section label with the glyph that says which section it is.
    fn section(&self, icon: IconName, title: &str, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        gpui_kit::base::h_flex()
            .w_full()
            .items_center()
            .gap_1p5()
            .child(
                Icon::new(icon)
                    .with_size(Size::Small)
                    .text_color(theme.colors.muted_foreground),
            )
            .child(
                div()
                    .text_size(theme.typography_tokens().xs.size)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.colors.muted_foreground)
                    .child(title.to_string()),
            )
            .into_any_element()
    }

    /// The friendly placeholder a panel shows instead of an empty scroll area.
    ///
    /// A blank body reads as a broken panel; the mascot plus one line of text
    /// says "nothing here yet" instead.
    fn empty_state(
        &self,
        media: IconName,
        title: &str,
        hint: &str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        Empty::new()
            .header(
                EmptyHeader::new()
                    .media(
                        EmptyMedia::new()
                            .with_variant(EmptyMediaVariant::Icon)
                            .child(
                                Icon::new(media)
                                    .with_size(Size::Large)
                                    .text_color(theme.colors.primary),
                            ),
                    )
                    .title(EmptyTitle::new().child(title.to_string()))
                    .description(
                        EmptyDescription::new()
                            .text_color(theme.colors.muted_foreground)
                            .child(hint.to_string()),
                    ),
            )
            .into_any_element()
    }

    /// The taller placeholder the chat and letters panels show, led by the
    /// mascot itself rather than an icon.
    fn mascot_state(&self, title: &str, hint: &str, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        Empty::new()
            .header(
                EmptyHeader::new()
                    .media(
                        EmptyMedia::new().child(
                            gpui_kit::img(WAVE)
                                .h(px(120.))
                                .w(px(62.))
                                .object_fit(gpui_kit::ObjectFit::Contain),
                        ),
                    )
                    .title(EmptyTitle::new().child(title.to_string()))
                    .description(
                        EmptyDescription::new()
                            .text_color(theme.colors.muted_foreground)
                            .child(hint.to_string()),
                    ),
            )
            .into_any_element()
    }

    /// A pill label, e.g. the presence line in the header or the days left on
    /// an anniversary.
    fn pill(&self, label: String, tinted: bool, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (background, foreground) = if tinted {
            (theme.colors.primary, theme.colors.primary_foreground)
        } else {
            (theme.colors.muted, theme.colors.muted_foreground)
        };
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_0p5()
            .rounded(theme.radius_full())
            .bg(background)
            .text_size(theme.typography_tokens().xs.size)
            .text_color(foreground)
            .child(label)
            .into_any_element()
    }

    /// The header: who this panel belongs to, and whether the partner is
    /// reachable.
    fn render_header(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let partner = self
            .core
            .engine
            .state
            .read(|s| s.partner.as_ref().map(|p| p.display_name.clone()))
            .flatten();
        let presence = self
            .core
            .engine
            .state
            .read(|s| s.partner_presence.clone())
            .flatten();
        let online = presence
            .as_ref()
            .is_some_and(|p| p.online && p.state != PresenceState::Dnd);
        let status = if partner.is_none() {
            self.t("settings.no_partner")
        } else if online {
            self.t("status.online")
        } else if presence
            .as_ref()
            .is_some_and(|p| p.state == PresenceState::Dnd)
        {
            self.t("status.dnd")
        } else {
            self.t("status.offline")
        };
        let dot = if online {
            theme.colors.success
        } else {
            theme.colors.muted_foreground
        };

        gpui_kit::base::h_flex()
            .w_full()
            .items_center()
            .gap_2p5()
            .px_3()
            .py_2p5()
            .bg(theme.colors.muted)
            .border_b_1()
            .border_color(theme.colors.border)
            .child(Avatar::new().src(PORTRAIT).with_size(Size::Size(px(40.))))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        div()
                            .text_size(theme.typography_tokens().md.size)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.colors.foreground)
                            .child(self.t("app.name").to_string()),
                    )
                    .child(
                        gpui_kit::base::h_flex()
                            .items_center()
                            .gap_1p5()
                            .min_w_0()
                            .child(div().size(px(7.)).rounded_full().bg(dot))
                            .child(
                                div()
                                    .text_size(theme.typography_tokens().xs.size)
                                    .text_color(theme.colors.muted_foreground)
                                    .child(match &partner {
                                        Some(name) => format!("{name} · {status}"),
                                        None => status.to_string(),
                                    }),
                            ),
                    ),
            )
            .child(
                Icon::new(IconName::Heart)
                    .with_size(Size::Medium)
                    .text_color(theme.colors.primary),
            )
            .into_any_element()
    }

    /// The tab strip: eight rounded tiles over two rows.
    ///
    /// A single-row tab bar cannot hold eight labelled tabs in a panel this
    /// narrow — the last ones fall off the edge — and the two-row grid keeps
    /// every section one click away at the minimum window size.
    fn render_tabs(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let language = self.language();
        let view = cx.entity().downgrade();
        let mut grid = v_flex()
            .w_full()
            .gap(px(6.))
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.colors.border);
        for row in TABS.chunks(4) {
            let mut line = gpui_kit::base::h_flex().w_full().gap(px(6.));
            for tab in row {
                let active = self.tab == *tab;
                let icon_color = if active {
                    theme.colors.primary_foreground
                } else {
                    theme.colors.muted_foreground
                };
                let label_color = icon_color;
                let view_click = view.clone();
                let index = tab.index();
                line = line.child(
                    v_flex()
                        .id(SharedString::from(format!("hub-tile-{index}")))
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .justify_center()
                        .gap(px(3.))
                        .py(px(7.))
                        .rounded(theme.radius_lg)
                        .text_color(label_color)
                        .when(active, |tile| tile.bg(theme.colors.primary))
                        .when(!active, |tile| {
                            tile.hover(|tile| tile.bg(theme.colors.muted))
                        })
                        .on_click(move |_, _, cx| {
                            let _ = view_click.update(cx, |view, cx| {
                                if let Some(tab) = TABS.get(index) {
                                    view.tab = *tab;
                                    view.core.save_hub_tab(tab.key());
                                    cx.notify();
                                }
                            });
                        })
                        .child(
                            Icon::new(tab.icon())
                                .with_size(Size::Small)
                                .text_color(icon_color),
                        )
                        .child(
                            div()
                                .text_size(theme.typography_tokens().xs.size)
                                .font_weight(if active {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .child(tab.title(&language).to_string()),
                        ),
                );
            }
            grid = grid.child(line);
        }
        grid.into_any_element()
    }

    fn render_chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let my_id = self
            .core
            .engine
            .state
            .read(|s| s.me.as_ref().map(|u| u.id.clone()))
            .flatten();
        let messages: Vec<insync_core::Message> = self
            .core
            .engine
            .state
            .read(|s| s.messages.iter().rev().take(60).rev().cloned().collect())
            .unwrap_or_default();
        let input = self.chat_input.clone();
        let view = cx.entity().downgrade();
        let empty = messages.is_empty();

        let mut list = v_flex()
            .flex_1()
            .gap_2()
            .children(messages.into_iter().map(|message| {
                let mine = my_id.as_ref().is_some_and(|id| message.sender_id == *id);
                // Mine is filled with the theme's rose, the partner's is an
                // outlined surface: the two are told apart by shape and colour
                // rather than by reading the sender each time.
                let bubble = Bubble::new()
                    .alignment(if mine {
                        MessageAlignment::End
                    } else {
                        MessageAlignment::Start
                    })
                    .with_variant(if mine {
                        BubbleVariant::Filled
                    } else {
                        BubbleVariant::Outline
                    })
                    .max_w(px(260.))
                    .child(
                        div()
                            .text_size(theme.typography_tokens().sm.size)
                            .child(message.body.clone()),
                    );
                v_flex()
                    .w_full()
                    .gap_1()
                    .items_center()
                    .when(mine, |row| row.items_end())
                    .when(!mine, |row| row.items_start())
                    .child(bubble)
                    .child(
                        div()
                            .px_1()
                            .text_size(theme.typography_tokens().xs.size)
                            .text_color(theme.colors.muted_foreground)
                            .child(clock_time(message.created_at)),
                    )
            }));
        if empty {
            list = list
                .items_center()
                .justify_center()
                .child(self.mascot_state(self.t("chat.empty"), self.t("chat.empty_hint"), cx));
        }

        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                div()
                    .id("chat-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(list),
            )
            .child(
                self.card(cx).child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&input)))
                        .child(
                            Button::new("chat-send")
                                .label(self.t("chat.send"))
                                .icon(IconName::Send)
                                .on_click({
                                    let view = view.clone();
                                    move |_, _window, cx| {
                                        let _ = view.update_in(cx, |view, window, cx| {
                                            view.send_chat(window, cx)
                                        });
                                    }
                                }),
                        ),
                ),
            )
            .into_any_element()
    }

    fn render_letters(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let my_id = self
            .core
            .engine
            .state
            .read(|s| s.me.as_ref().map(|u| u.id.clone()))
            .flatten();
        let letters = self
            .core
            .engine
            .state
            .read(|s| s.letters.clone())
            .unwrap_or_default();
        let input = self.letter_input.clone();
        let view = cx.entity().downgrade();
        let empty = letters.is_empty();
        let mut list = v_flex()
            .flex_1()
            .gap_2()
            .children(letters.into_iter().map(|letter| {
                let unread = my_id
                    .as_ref()
                    .is_some_and(|id| letter.sender_id != *id && letter.read_at.is_none());
                let view_read = view.clone();
                let letter_id = letter.id.clone();
                let mine = my_id.as_ref().is_some_and(|id| letter.sender_id == *id);
                v_flex()
                    .w_full()
                    .gap_2()
                    .p_3()
                    .rounded(theme.radius_lg)
                    .bg(if unread {
                        theme.colors.accent
                    } else {
                        theme.colors.popover
                    })
                    .border_1()
                    .border_color(if unread {
                        theme.colors.primary
                    } else {
                        theme.colors.border
                    })
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                gpui_kit::base::h_flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        Icon::new(IconName::MailOpen)
                                            .with_size(Size::Small)
                                            .text_color(theme.colors.muted_foreground),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.typography_tokens().xs.size)
                                            .text_color(theme.colors.muted_foreground)
                                            .child(format!(
                                                "{} {}",
                                                if mine {
                                                    self.t("letters.from_me")
                                                } else {
                                                    self.t("letters.from_partner")
                                                },
                                                clock_time(letter.created_at)
                                            )),
                                    ),
                            )
                            .when(unread, |row| {
                                row.child(self.pill(self.t("letters.unread").to_string(), true, cx))
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.typography_tokens().sm.size)
                            .text_color(theme.colors.foreground)
                            .child(letter.body.clone()),
                    )
                    .when(unread, |card| {
                        card.child(
                            gpui_kit::base::h_flex().w_full().justify_end().child(
                                Button::new(SharedString::from(format!("read-{letter_id}")))
                                    .label(self.t("letters.mark_read"))
                                    .compact()
                                    .on_click(move |_, _, cx| {
                                        let id = letter_id.clone();
                                        let _ = view_read.update_in(cx, |v, _window, _cx| {
                                            v.core.engine.command(SyncCommand::Enqueue(
                                                SyncOp::MarkLetterRead { letter_id: id },
                                            ));
                                        });
                                    }),
                            ),
                        )
                    })
            }));
        if empty {
            list = list
                .items_center()
                .justify_center()
                .child(self.mascot_state(
                    self.t("letters.empty"),
                    self.t("letters.empty_hint"),
                    cx,
                ));
        }

        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                self.card(cx).child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&input)))
                        .child(
                            Button::new("letter-send")
                                .label(self.t("letters.leave"))
                                .icon(IconName::MailOpen)
                                .on_click({
                                    let view = view.clone();
                                    move |_, _window, cx| {
                                        let _ = view.update_in(cx, |view, window, cx| {
                                            view.send_letter(window, cx)
                                        });
                                    }
                                }),
                        ),
                ),
            )
            .child(
                div()
                    .id("letter-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    fn render_focus(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let focus = self
            .core
            .engine
            .state
            .read(|s| s.active_focus.clone())
            .flatten();
        let me_id = self
            .core
            .engine
            .state
            .read(|s| s.me.as_ref().map(|u| u.id.clone()))
            .flatten();
        let view = cx.entity().downgrade();
        let mut panel = v_flex().flex_1().gap_2().p_2();
        if let Some(session) = focus {
            let remaining = session.remaining_ms(now_ms()).unwrap_or(0);
            let minutes = remaining / 60_000;
            let seconds = (remaining % 60_000) / 1000;
            let initiator = me_id.as_ref().is_some_and(|id| id == &session.initiator_id);
            let total_ms = session.duration_secs * 1000;
            let elapsed = (total_ms - remaining).max(0);
            let progress = if total_ms > 0 {
                (elapsed as f32 / total_ms as f32) * 100.
            } else {
                0.
            };
            let (state_label, state_tint) = match session.state {
                FocusState::Invited => (self.t("focus.invited"), theme.colors.warning),
                FocusState::Paused => (self.t("focus.paused"), theme.colors.muted_foreground),
                _ => (self.t("focus.active"), theme.colors.success),
            };
            let mut actions = gpui_kit::base::h_flex().gap_2().flex_wrap();
            match session.state {
                FocusState::Invited => {
                    if !initiator {
                        actions = actions
                            .child(
                                Button::new("focus-accept")
                                    .label(self.t("focus.accept"))
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |v, window, cx| {
                                                v.focus_action(
                                                    window,
                                                    insync_core::FocusAction::Accept,
                                                    cx,
                                                )
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("focus-decline")
                                    .label(self.t("focus.decline"))
                                    .outline()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |v, window, cx| {
                                                v.focus_action(
                                                    window,
                                                    insync_core::FocusAction::Decline,
                                                    cx,
                                                )
                                            });
                                        }
                                    }),
                            );
                    } else {
                        actions = actions.child(
                            Button::new("focus-cancel")
                                .label(self.t("focus.cancel"))
                                .outline()
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, window, cx| {
                                            v.focus_action(
                                                window,
                                                insync_core::FocusAction::Cancel,
                                                cx,
                                            )
                                        });
                                    }
                                }),
                        );
                    }
                }
                FocusState::Active => {
                    actions = actions
                        .child(
                            Button::new("focus-pause")
                                .label(self.t("focus.pause"))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, window, cx| {
                                            v.focus_action(
                                                window,
                                                insync_core::FocusAction::Pause,
                                                cx,
                                            )
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("focus-finish")
                                .label(self.t("focus.finish"))
                                .outline()
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, window, cx| {
                                            v.focus_action(
                                                window,
                                                insync_core::FocusAction::FinishEarly,
                                                cx,
                                            )
                                        });
                                    }
                                }),
                        );
                }
                FocusState::Paused => {
                    actions = actions.child(
                        Button::new("focus-resume")
                            .label(self.t("focus.resume"))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |v, window, cx| {
                                        v.focus_action(window, insync_core::FocusAction::Resume, cx)
                                    });
                                }
                            }),
                    );
                }
                _ => {}
            }
            // The ring carries the session's progress, so the remaining time
            // can be read at a glance without doing arithmetic on the clock.
            let timer = ProgressCircle::new("focus-progress")
                .value(progress)
                .color(theme.colors.primary)
                .child(
                    div()
                        .text_size(px(34.))
                        .font_family(theme.mono_font_family.clone())
                        .font_weight(FontWeight::BOLD)
                        .text_color(theme.colors.foreground)
                        .child(format!("{minutes:02}:{seconds:02}")),
                );
            panel = panel.child(
                self.card(cx)
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .py_4()
                    .child(timer)
                    .child(
                        gpui_kit::base::h_flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(7.)).rounded_full().bg(state_tint))
                            .child(
                                div()
                                    .text_size(theme.typography_tokens().xs.size)
                                    .text_color(theme.colors.muted_foreground)
                                    .child(state_label.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.typography_tokens().sm.size)
                            .text_color(theme.colors.foreground)
                            .child(session.topic.clone().unwrap_or_default()),
                    )
                    .when(session.state == FocusState::Invited, |card| {
                        card.child(
                            div()
                                .text_size(theme.typography_tokens().xs.size)
                                .text_color(theme.colors.muted_foreground)
                                .child(if initiator {
                                    self.t("focus.waiting_partner")
                                } else {
                                    self.t("focus.waiting")
                                }),
                        )
                    })
                    .child(actions),
            );
        } else {
            let topic_input = self.focus_topic_input.clone();
            let durations = [25i64, 45, 50, 90];
            let chosen = self.focus_duration;
            panel = panel.child(
                self.card(cx)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.section(IconName::Timer, self.t("focus.start"), cx))
                    .child(div().child(Input::new(&topic_input)))
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .gap_2()
                            .children(durations.map(|mins| {
                                let view = view.clone();
                                let active = chosen == mins * 60;
                                v_flex()
                                    .id(SharedString::from(format!("dur-{mins}")))
                                    .flex_1()
                                    .items_center()
                                    .py_2()
                                    .rounded(theme.radius_lg)
                                    .bg(if active {
                                        theme.colors.primary
                                    } else {
                                        theme.colors.muted
                                    })
                                    .text_color(if active {
                                        theme.colors.primary_foreground
                                    } else {
                                        theme.colors.foreground
                                    })
                                    .when(!active, |tile| {
                                        tile.hover(|tile| tile.bg(theme.colors.accent))
                                    })
                                    .on_click(move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.focus_duration = mins * 60;
                                            cx.notify();
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_size(theme.typography_tokens().sm.size)
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(mins.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.typography_tokens().xs.size)
                                            .child(self.t("focus.minutes")),
                                    )
                            })),
                    )
                    .child(
                        Button::new("focus-invite")
                            .label(self.t("focus.invite"))
                            .icon(IconName::HeartHandshake)
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |view, window, cx| {
                                        view.invite_focus(window, cx)
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme.typography_tokens().xs.size)
                            .text_color(theme.colors.muted_foreground)
                            .child(self.t("focus.hint")),
                    ),
            );
        }
        panel.into_any_element()
    }

    fn render_todos(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let personal = self.core.todos_store.load();
        let shared = self
            .core
            .engine
            .state
            .read(|s| s.shared_todos.clone())
            .unwrap_or_default();
        let input = self.todo_input.clone();
        let view = cx.entity().downgrade();
        let empty = personal.todos.is_empty() && shared.is_empty();
        let mut list = v_flex().flex_1().gap_1p5();
        list = list.child(self.section(IconName::ListTodo, self.t("todos.personal"), cx));
        for todo in personal.todos.iter().take(30) {
            let id = todo.id.clone();
            let done = todo.completed_at.is_some();
            let view_toggle = view.clone();
            let view_delete = view.clone();
            let id_toggle = id.clone();
            let id_delete = id.clone();
            list = list.child(
                self.card(cx).child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            // A checkbox, not a ✓ button: the state is
                            // visible at a glance and the whole row reads
                            // as one control.
                            Checkbox::new(SharedString::from(format!("todo-{id}")))
                                .checked(done)
                                .label(SharedString::from(todo.title.clone()))
                                .on_click(move |checked, _window, cx| {
                                    let _ = view_toggle.update_in(cx, |v, window, cx| {
                                        let _ = checked;
                                        v.toggle_personal_todo(window, &id_toggle, cx)
                                    });
                                }),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new(SharedString::from(format!("todo-del-{id}")))
                                .icon(IconName::Trash)
                                .tooltip(self.t("hub.delete"))
                                .compact()
                                .ghost()
                                .on_click(move |_, _window, cx| {
                                    let _ = view_delete.update_in(cx, |v, window, cx| {
                                        v.delete_personal_todo(window, &id_delete, cx)
                                    });
                                }),
                        ),
                ),
            );
        }
        list = list.child(self.section(IconName::Users, self.t("todos.shared"), cx));
        for todo in shared.iter().take(30) {
            let id = todo.id.clone();
            let done = todo.completed_at.is_some();
            let view_click = view.clone();
            list = list.child(
                self.card(cx).child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            Checkbox::new(SharedString::from(format!("shared-todo-{id}")))
                                .checked(done)
                                .label(SharedString::from(todo.title.clone()))
                                .on_click(move |_, _window, cx| {
                                    let id = id.clone();
                                    let _ = view_click.update_in(cx, |v, window, cx| {
                                        v.toggle_shared_todo(window, id, done, cx)
                                    });
                                }),
                        )
                        .child(div().flex_1()),
                ),
            );
        }
        if empty {
            list = list.items_center().justify_center().child(self.empty_state(
                IconName::ListTodo,
                self.t("todos.empty"),
                self.t("todos.empty_hint"),
                cx,
            ));
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                self.card(cx)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().child(Input::new(&input)))
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                Button::new("todo-add-personal")
                                    .label(self.t("todos.add"))
                                    .icon(IconName::Plus)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |view, window, cx| {
                                                view.add_personal_todo(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("todo-add-shared")
                                    .label(self.t("todos.add_shared"))
                                    .icon(IconName::Users)
                                    .outline()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |view, window, cx| {
                                                view.add_shared_todo(window, cx)
                                            });
                                        }
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .id("todo-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    fn render_notes(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let personal = self.core.notes_store.load();
        let shared = self
            .core
            .engine
            .state
            .read(|s| s.shared_notes.clone())
            .unwrap_or_default();
        let input = self.note_input.clone();
        let view = cx.entity().downgrade();
        let empty = personal.notes.is_empty() && shared.is_empty();
        let mut rows = v_flex().flex_1().gap_1p5();
        rows = rows.child(self.section(IconName::StickyNote, self.t("notes.personal"), cx));
        for note in personal.notes.iter().take(20) {
            let id = note.id.clone();
            let view_select = view.clone();
            let selected = self.personal_note_selected.as_ref() == Some(&note.id);
            rows = rows.child(
                self.card(cx)
                    .when(selected, |card| {
                        card.border_color(theme.colors.primary)
                            .bg(theme.colors.accent)
                    })
                    .child(
                        gpui_kit::base::h_flex()
                            .id(SharedString::from(format!("pnote-{id}")))
                            .w_full()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                let _ = view_select.update_in(cx, |v, window, cx| {
                                    v.personal_note_selected = Some(id.clone());
                                    if let Some(content) = v
                                        .core
                                        .notes_store
                                        .load()
                                        .notes
                                        .iter()
                                        .find(|n| n.id == id)
                                        .map(|n| n.content.clone())
                                    {
                                        v.note_input.update(cx, |state, cx| {
                                            state.set_value(content, window, cx)
                                        });
                                    }
                                });
                            })
                            .child(
                                Icon::new(if selected {
                                    IconName::Pencil
                                } else {
                                    IconName::StickyNote
                                })
                                .with_size(Size::Small)
                                .text_color(theme.colors.muted_foreground),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(theme.typography_tokens().sm.size)
                                    .text_color(theme.colors.foreground)
                                    .child(note.content.chars().take(24).collect::<String>()),
                            ),
                    ),
            );
        }
        rows = rows.child(self.section(IconName::Users, self.t("notes.shared"), cx));
        for note in shared.iter().take(20) {
            let id = note.id.clone();
            let view_select = view.clone();
            let selected = self.shared_note_selected.as_ref() == Some(&note.id);
            rows = rows.child(
                self.card(cx)
                    .when(selected, |card| {
                        card.border_color(theme.colors.primary)
                            .bg(theme.colors.accent)
                    })
                    .child(
                        gpui_kit::base::h_flex()
                            .id(SharedString::from(format!("snote-{id}")))
                            .w_full()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                let _ = view_select.update_in(cx, |v, window, cx| {
                                    v.shared_note_selected = Some(id.clone());
                                    if let Some(content) = v
                                        .core
                                        .engine
                                        .state
                                        .read(|s| {
                                            s.shared_notes
                                                .iter()
                                                .find(|n| n.id == id)
                                                .map(|n| n.content.clone())
                                        })
                                        .flatten()
                                    {
                                        v.note_input.update(cx, |state, cx| {
                                            state.set_value(content, window, cx)
                                        });
                                    }
                                });
                            })
                            .child(
                                Icon::new(IconName::Users)
                                    .with_size(Size::Small)
                                    .text_color(theme.colors.muted_foreground),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(theme.typography_tokens().sm.size)
                                    .text_color(theme.colors.foreground)
                                    .child(note.content.chars().take(20).collect::<String>()),
                            )
                            .child(self.pill(format!("rev {}", note.revision), false, cx)),
                    ),
            );
        }
        if empty {
            rows = rows.items_center().justify_center().child(self.empty_state(
                IconName::StickyNote,
                self.t("notes.empty"),
                self.t("notes.empty_hint"),
                cx,
            ));
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                self.card(cx)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().child(Input::new(&input)))
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                Button::new("pnote-add")
                                    .label(self.t("notes.add"))
                                    .icon(IconName::Plus)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |view, window, cx| {
                                                view.add_personal_note(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("pnote-save")
                                    .label(self.t("notes.save_personal"))
                                    .outline()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |view, window, cx| {
                                                view.save_personal_note(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("snote-add")
                                    .label(self.t("notes.new_shared"))
                                    .icon(IconName::Users)
                                    .outline()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |view, window, cx| {
                                                view.add_shared_note(window, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("snote-save")
                                    .label(self.t("notes.save_shared"))
                                    .outline()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |view, window, cx| {
                                                view.save_shared_note(window, cx)
                                            });
                                        }
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .id("note-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(rows),
            )
            .into_any_element()
    }

    fn render_anniversaries(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let now = jiff::Zoned::now();
        let personal = self.core.anniversaries_store.load();
        let shared = self
            .core
            .engine
            .state
            .read(|s| s.shared_anniversaries.clone())
            .unwrap_or_default();
        let name_input = self.anniversary_name_input.clone();
        let date_input = self.anniversary_date_input.clone();
        let view = cx.entity().downgrade();
        let empty = personal.anniversaries.is_empty() && shared.is_empty();
        let mut rows = v_flex().flex_1().gap_1p5();
        rows = rows.child(self.section(IconName::Heart, self.t("days.personal"), cx));
        for item in personal.anniversaries.iter().take(20) {
            rows = rows.child(self.anniversary_row(
                &item.name,
                &item.date,
                item.repeat_yearly,
                &now,
                cx,
            ));
        }
        rows = rows.child(self.section(IconName::Users, self.t("days.shared"), cx));
        for item in shared.iter().take(20) {
            rows = rows.child(self.anniversary_row(
                &item.name,
                &item.date,
                item.repeat_yearly,
                &now,
                cx,
            ));
        }
        if empty {
            rows = rows.items_center().justify_center().child(self.empty_state(
                IconName::CalendarHeart,
                self.t("days.empty"),
                self.t("days.empty_hint"),
                cx,
            ));
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                self.card(cx)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.section(IconName::CalendarHeart, self.t("days.add"), cx))
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(Input::new(&name_input)))
                            .child(div().w(px(110.)).child(Input::new(&date_input))),
                    )
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .gap_2()
                            .child(
                                Button::new("ann-add-personal")
                                    .label(self.t("days.add_personal"))
                                    .icon(IconName::Plus)
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |v, window, cx| {
                                                v.add_anniversary(window, false, cx)
                                            });
                                        }
                                    }),
                            )
                            .child(
                                Button::new("ann-add-shared")
                                    .label(self.t("days.add_shared"))
                                    .icon(IconName::Users)
                                    .outline()
                                    .on_click({
                                        let view = view.clone();
                                        move |_, _, cx| {
                                            let _ = view.update_in(cx, |v, window, cx| {
                                                v.add_anniversary(window, true, cx)
                                            });
                                        }
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.typography_tokens().xs.size)
                            .text_color(theme.colors.muted_foreground)
                            .child(self.t("days.format_hint")),
                    ),
            )
            .child(
                div()
                    .id("ann-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(rows),
            )
            .into_any_element()
    }

    /// One anniversary: the name, the date, and how far away it is.
    ///
    /// The countdown is a filled pill when the date has arrived or is close,
    /// so a birthday this week stands out from one six months out.
    fn anniversary_row(
        &self,
        name: &str,
        date: &str,
        repeat_yearly: bool,
        now: &jiff::Zoned,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let days = clock::anniversary_days(date, repeat_yearly, now).unwrap_or(0);
        let soon = days.abs() <= 7;
        self.card(cx)
            .child(
                gpui_kit::base::h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        Icon::new(if soon {
                            IconName::PartyPopper
                        } else {
                            IconName::CalendarHeart
                        })
                        .with_size(Size::Small)
                        .text_color(if soon {
                            theme.colors.primary
                        } else {
                            theme.colors.muted_foreground
                        }),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_size(theme.typography_tokens().sm.size)
                                    .text_color(theme.colors.foreground)
                                    .child(name.to_string()),
                            )
                            .child(
                                div()
                                    .text_size(theme.typography_tokens().xs.size)
                                    .text_color(theme.colors.muted_foreground)
                                    .child(date.to_string()),
                            ),
                    )
                    .child(
                        // "in 12 days" / "12 days ago" reads better than a
                        // signed number.
                        self.pill(
                            if days >= 0 {
                                format!(
                                    "{} {} {}",
                                    self.t("days.in"),
                                    days,
                                    self.t("days.days_left")
                                )
                            } else {
                                format!("{} {}", days.abs(), self.t("days.ago"))
                            },
                            soon,
                            cx,
                        ),
                    ),
            )
            .into_any_element()
    }

    fn render_alarms(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let now = jiff::Zoned::now();
        let alarms = self.core.alarms_store.load();
        let title_input = self.alarm_title_input.clone();
        let time_input = self.alarm_time_input.clone();
        let view = cx.entity().downgrade();
        let empty = alarms.alarms.is_empty();
        let mut rows = v_flex().flex_1().gap_1p5();
        for alarm in alarms.alarms.iter().take(30) {
            let id = alarm.id.clone();
            let view_toggle = view.clone();
            let view_delete = view.clone();
            let id_toggle = id.clone();
            let id_delete = id.clone();
            let next = clock::next_trigger(alarm, &now)
                .map(|t| clock::format_time(&t, true))
                .unwrap_or_else(|| "—".to_string());
            let repeat = match &alarm.repeat {
                AlarmRepeat::Once(date) => format!("{} {date}", self.t("alarms.once")),
                AlarmRepeat::Daily => self.t("alarms.daily").to_string(),
                AlarmRepeat::Weekdays(days) if days.is_empty() => {
                    self.t("alarms.daily").to_string()
                }
                AlarmRepeat::Weekdays(days) => {
                    let language = self.language();
                    days.iter()
                        .filter_map(|day| {
                            let key = format!("weekday.{day}");
                            let name = text(&language, &key);
                            (!name.is_empty()).then(|| name.to_string())
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                }
            };
            rows = rows.child(
                self.card(cx)
                    .when(!alarm.enabled, |card| card.bg(theme.colors.muted))
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .items_center()
                            .gap_3()
                            .child(
                                Icon::new(if alarm.enabled {
                                    IconName::BellRing
                                } else {
                                    IconName::Bell
                                })
                                .with_size(Size::Medium)
                                .text_color(if alarm.enabled {
                                    theme.colors.primary
                                } else {
                                    theme.colors.muted_foreground
                                }),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(
                                        gpui_kit::base::h_flex()
                                            .items_baseline()
                                            .gap_2()
                                            .child(
                                                // The time is the thing the
                                                // user is scanning for, so it
                                                // is the largest type on the
                                                // card and the title rides
                                                // along as a caption.
                                                div()
                                                    .text_size(px(20.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(if alarm.enabled {
                                                        theme.colors.foreground
                                                    } else {
                                                        theme.colors.muted_foreground
                                                    })
                                                    .child(alarm.time.clone()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(theme.typography_tokens().sm.size)
                                                    .text_color(theme.colors.foreground)
                                                    .child(alarm.title.clone()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.typography_tokens().xs.size)
                                            .text_color(theme.colors.muted_foreground)
                                            .child(format!(
                                                "{repeat} · {} {next}",
                                                self.t("alarms.next")
                                            )),
                                    ),
                            )
                            .child(
                                Switch::new(SharedString::from(format!("alarm-{id}")))
                                    .checked(alarm.enabled)
                                    .tooltip(if alarm.enabled {
                                        self.t("alarms.disable")
                                    } else {
                                        self.t("alarms.enable")
                                    })
                                    .on_click(move |_, _window, cx| {
                                        let _ = view_toggle.update_in(cx, |v, window, cx| {
                                            v.toggle_alarm(window, &id_toggle, cx)
                                        });
                                    }),
                            )
                            .child(
                                Button::new(SharedString::from(format!("alarm-del-{id}")))
                                    .icon(IconName::Trash)
                                    .tooltip(self.t("hub.delete"))
                                    .compact()
                                    .ghost()
                                    .on_click(move |_, _, cx| {
                                        let _ = view_delete.update_in(cx, |v, window, cx| {
                                            v.delete_alarm(window, &id_delete, cx)
                                        });
                                    }),
                            ),
                    ),
            );
        }
        if empty {
            rows = rows.items_center().justify_center().child(self.empty_state(
                IconName::AlarmClock,
                self.t("alarms.empty"),
                self.t("alarms.empty_hint"),
                cx,
            ));
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                self.card(cx)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.section(IconName::AlarmClock, self.t("alarms.add"), cx))
                    .child(
                        gpui_kit::base::h_flex()
                            .w_full()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(Input::new(&title_input)))
                            .child(div().w(px(104.)).child(Input::new(&time_input))),
                    )
                    .child(
                        Button::new("alarm-add")
                            .label(self.t("alarms.add"))
                            .icon(IconName::Plus)
                            .on_click(move |_, _, cx| {
                                let _ = view
                                    .update_in(cx, |view, window, cx| view.add_alarm(window, cx));
                            }),
                    ),
            )
            .child(
                div()
                    .id("alarm-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(rows),
            )
            .into_any_element()
    }

    fn render_settings(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let account = self.core.account_store.load();
        let partner = self.core.engine.state.read(|s| s.partner.clone()).flatten();
        let status = self.core.status_text();
        let zh = self.language() == "zh-CN";
        let server_input = self.server_input.clone();
        let username_input = self.username_input.clone();
        let password_input = self.password_input.clone();
        let invite_input = self.invite_input.clone();
        let view = cx.entity().downgrade();
        let signed_in = !account.token.is_empty();

        // Account: who the pet belongs to, and the sign-in or pair-up actions
        // that go with that identity.
        let mut account_card = self.card(cx).gap_2().child(self.section(
            IconName::CircleUser,
            self.t("settings.account"),
            cx,
        ));
        let who = if account.display_name.is_empty() {
            self.t("settings.no_account").to_string()
        } else {
            account.display_name.clone()
        };
        account_card = account_card.child(
            gpui_kit::base::h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(
                    Avatar::new()
                        .name(who.clone())
                        .with_size(Size::Small)
                        .when(signed_in, |avatar| avatar.src(PORTRAIT)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            div()
                                .text_size(theme.typography_tokens().sm.size)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.colors.foreground)
                                .child(who),
                        )
                        .child(
                            div()
                                .text_size(theme.typography_tokens().xs.size)
                                .text_color(theme.colors.muted_foreground)
                                .child(status),
                        ),
                ),
        );
        if signed_in {
            account_card = account_card
                .child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&invite_input)))
                        .child(
                            Button::new("invite-accept")
                                .label(self.t("settings.accept_invite"))
                                .icon(IconName::HeartHandshake)
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::AcceptInvite, cx)
                                        });
                                    }
                                }),
                        ),
                )
                .child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            Button::new("invite-create")
                                .label(self.t("settings.invite_code"))
                                .icon(IconName::Ticket)
                                .outline()
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Invite, cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("dissolve")
                                .label(self.t("settings.dissolve"))
                                .icon(IconName::HeartCrack)
                                .outline()
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Dissolve, cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("logout")
                                .label(self.t("settings.logout"))
                                .icon(IconName::LogOut)
                                .ghost()
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Logout, cx)
                                        });
                                    }
                                }),
                        ),
                );
        } else {
            account_card = account_card
                .child(div().child(Input::new(&server_input)))
                .child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(Input::new(&username_input)))
                        .child(div().flex_1().min_w_0().child(Input::new(&password_input))),
                )
                .child(
                    gpui_kit::base::h_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            Button::new("login")
                                .label(self.t("settings.login"))
                                .icon(IconName::LogIn)
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Login, cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("register")
                                .label(self.t("settings.register"))
                                .icon(IconName::UserPlus)
                                .outline()
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Register, cx)
                                        });
                                    }
                                }),
                        ),
                );
        }

        // Partner: the other half of the product, and the one action that
        // reaches them from the settings page.
        let mut partner_card = self.card(cx).gap_2().child(self.section(
            IconName::Users,
            self.t("settings.partner"),
            cx,
        ));
        partner_card = partner_card.child(match &partner {
            Some(partner) => gpui_kit::base::h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .child(
                    Avatar::new()
                        .name(partner.display_name.clone())
                        .with_size(Size::Small),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(theme.typography_tokens().sm.size)
                        .text_color(theme.colors.foreground)
                        .child(partner.display_name.clone()),
                )
                .child(
                    Button::new("poke-here")
                        .label(self.t("tray.poke"))
                        .icon(IconName::HandHeart)
                        .outline()
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                let _ = view
                                    .update_in(cx, |view, window, cx| view.set_poke(window, cx));
                            }
                        }),
                ),
            None => div()
                .text_size(theme.typography_tokens().sm.size)
                .text_color(theme.colors.muted_foreground)
                .child(self.t("settings.no_partner").to_string()),
        });

        // Preferences: language, plus the one promise about personal data.
        let preferences = self
            .card(cx)
            .gap_2()
            .child(self.section(IconName::Settings2, self.t("settings.language"), cx))
            .child(
                gpui_kit::base::h_flex()
                    .w_full()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("lang-switch")
                            .label(if zh { "English" } else { "中文" })
                            .icon(IconName::Languages)
                            .outline()
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |v, window, cx| {
                                        let language = if zh { "en" } else { "zh-CN" };
                                        v.set_language(language, window, cx);
                                    });
                                    cx.refresh_windows();
                                }
                            }),
                    ),
            )
            .child(
                gpui_kit::base::h_flex()
                    .w_full()
                    .items_start()
                    .gap_1p5()
                    .child(
                        Icon::new(IconName::Lock)
                            .with_size(Size::Small)
                            .text_color(theme.colors.muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme.typography_tokens().xs.size)
                            .text_color(theme.colors.muted_foreground)
                            .child(self.t("settings.local_hint").to_string()),
                    ),
            );

        v_flex()
            .id("settings-scroll")
            .flex_1()
            .w_full()
            .gap_2()
            .p_3()
            .overflow_y_scroll()
            .child(account_card)
            .child(partner_card)
            .child(preferences)
            .into_any_element()
    }
}

enum AccountOp {
    Register,
    Login,
    Invite,
    AcceptInvite,
    Dissolve,
    Logout,
}

/// Thin accessor so the background thread can clear the account store.
struct JsonStoreAccount<'a>(&'a crate::app::AppCore);

impl JsonStoreAccount<'_> {
    fn clear(&self) -> Result<(), crate::store::StoreError> {
        self.0.account_store.clear()
    }
}

fn save_account(
    core: &Arc<crate::app::AppCore>,
    user_id: &str,
    username: &str,
    display_name: &str,
    token: &str,
) {
    let account = crate::store::Account {
        schema_version: crate::store::SCHEMA_VERSION,
        user_id: user_id.to_string(),
        username: username.to_string(),
        display_name: display_name.to_string(),
        token: token.to_string(),
        saved_at: now_ms(),
    };
    let _ = core.account_store.save(&account);
    core.engine.command(SyncCommand::Connect);
}

impl gpui_kit::Render for HubView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let content = match self.tab {
            HubTab::Chat => self.render_chat(cx),
            HubTab::Letters => self.render_letters(cx),
            HubTab::Focus => self.render_focus(cx),
            HubTab::Todos => self.render_todos(cx),
            HubTab::Notes => self.render_notes(cx),
            HubTab::Anniversaries => self.render_anniversaries(cx),
            HubTab::Alarms => self.render_alarms(cx),
            HubTab::Settings => self.render_settings(cx),
        };
        v_flex()
            .w_full()
            .h_full()
            .bg(theme.colors.background)
            .child(self.render_header(cx))
            .child(self.render_tabs(cx))
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The menu item says "Open Panel", so it opens the panel the app already
    /// created at startup instead of closing it.
    #[test]
    fn open_panel_never_closes_an_existing_panel() {
        assert_eq!(intent(HubGesture::Show, Some(true)), HubIntent::Open);
        assert_eq!(intent(HubGesture::Show, Some(false)), HubIntent::Open);
        assert_eq!(intent(HubGesture::Show, None), HubIntent::Open);
    }

    /// The pet's click closes a panel the user can actually see, and raises one
    /// that is buried behind other windows instead of making it disappear.
    #[test]
    fn pet_click_closes_only_a_panel_that_is_in_front() {
        assert_eq!(intent(HubGesture::Toggle, Some(true)), HubIntent::Close);
        assert_eq!(intent(HubGesture::Toggle, Some(false)), HubIntent::Open);
    }

    /// A window the user closed with its close button leaves no state to act on,
    /// so the next gesture has to build a fresh window rather than do nothing.
    #[test]
    fn a_gesture_without_a_live_window_opens_one() {
        assert_eq!(intent(HubGesture::Toggle, None), HubIntent::Open);
    }

    /// The stored name of a section is the same string in both places it is
    /// written down: the `tab.<key>` label and the saved panel state. A typo in
    /// either would send a returning user to the wrong section.
    #[test]
    fn every_section_round_trips_through_its_stored_name() {
        for tab in TABS {
            assert_eq!(HubTab::from_key(tab.key()), Some(tab));
            assert!(
                !text("zh-CN", &format!("tab.{}", tab.key())).is_empty()
                    && !text("en", &format!("tab.{}", tab.key())).is_empty(),
                "tab.{} has no label",
                tab.key()
            );
        }
        assert_eq!(HubTab::from_key("nope"), None);
    }

    /// Every label the panel draws resolves in both languages.
    ///
    /// A key that is missing or misspelled falls back to an empty string, so
    /// the button or tab it belongs to renders blank instead of failing the
    /// build. The keys are read out of this file's own literals — anything
    /// shaped like `section.name` — so a new label is covered the moment it is
    /// written, without a list to maintain.
    #[test]
    fn every_label_the_panel_writes_exists_in_both_languages() {
        let source = include_str!("hub.rs");
        let mut keys: Vec<&str> = source
            .split('"')
            .skip(1)
            .step_by(2)
            .filter(|literal| looks_like_a_key(literal))
            .collect();
        keys.sort_unstable();
        keys.dedup();
        assert!(
            keys.len() >= 50,
            "the scan found only {} keys, so it is no longer looking at the labels",
            keys.len()
        );
        for key in keys {
            for language in ["zh-CN", "en"] {
                assert!(
                    !text(language, key).is_empty(),
                    "\"{key}\" has no {language} label"
                );
            }
        }
        for day in 0..7 {
            let key = format!("weekday.{day}");
            for language in ["zh-CN", "en"] {
                assert!(
                    !text(language, &key).is_empty(),
                    "\"{key}\" has no {language} label"
                );
            }
        }
    }

    /// A catalog key: `section.name`, lower case, no spaces or paths. Element
    /// ids (`hub-tile-0`), sizes (`120`) and asset paths are not keys, and the
    /// section has to be one the catalog actually has.
    fn looks_like_a_key(literal: &str) -> bool {
        const SECTIONS: [&str; 14] = [
            "app", "tab", "tray", "chat", "letters", "focus", "todos", "notes", "days", "alarms",
            "settings", "status", "notify", "weekday",
        ];
        let mut parts = literal.split('.');
        let (Some(section), Some(name), None) = (parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        SECTIONS.contains(&section)
            && [section, name].into_iter().all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            })
    }
}
