//! The hub panel: one GPUI window with tabs for chat, letters, focus,
//! todos, notes, anniversaries, alarms and settings. Independent of the
//! pet window; opening/closing it never touches the animation.

use crate::app::Coordinator;
use crate::clock;
use crate::i18n::text;
use crate::store::{Alarm, AlarmRepeat, PersonalAnniversary, PersonalNote, PersonalTodo};
use gpui_kit::base::v_flex;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::tab::{Tab as GpuiTab, TabBar};
use gpui_kit::component::{ActiveTheme, Root};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, Context, Entity, TitlebarOptions, Window, WindowBounds,
    WindowHandle, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_kit::{FontWeight, SharedString};
use insync_client::SyncCommand;
use insync_core::{FocusState, PresenceState, SyncOp};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

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

/// Opens the hub window and stores the handle in the coordinator.
pub fn ensure_hub(cx: &mut App) {
    let Some(coordinator) = cx.try_global::<Coordinator>() else {
        return;
    };
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
        let view = cx.new(|cx| HubView::new(&core, window, cx));
        if !cx.has_global::<gpui_kit::component::Theme>() {
            gpui_kit::init(cx);
        }
        *view_slot_for_builder.borrow_mut() = Some(view.clone());
        cx.new(|cx| Root::new(view, window, cx))
    });
    if let Ok(window) = opened {
        let has_view = view_slot.borrow().is_some();
        if has_view && let Some(coordinator) = cx.try_global::<Coordinator>() {
            *coordinator
                .hub
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(HubWindowHandle { window });
        }
    }
}

/// Closes the hub if open; reopening creates a fresh view.
pub fn toggle_hub(cx: &mut App) {
    let existing = cx.try_global::<Coordinator>().and_then(|coordinator| {
        coordinator
            .hub
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    });
    if let Some(handle) = existing {
        let _ = handle.window.update(cx, |_, window, _| {
            window.remove_window();
        });
        return;
    }
    ensure_hub(cx);
}

impl HubView {
    fn new(core: &Arc<crate::app::AppCore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input =
            |window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx));
        let input_with =
            |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
                cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
            };
        Self {
            core: Arc::clone(core),
            tab: HubTab::Chat,
            chat_input: input_with("chat.placeholder", window, cx),
            letter_input: input_with("letters.placeholder", window, cx),
            todo_input: input_with("todos.add", window, cx),
            note_input: input(window, cx),
            anniversary_name_input: input_with("days.add", window, cx),
            anniversary_date_input: input(window, cx),
            alarm_title_input: input_with("alarms.add", window, cx),
            alarm_time_input: input_with("", window, cx),
            server_input: input_with("settings.server", window, cx),
            username_input: input_with("", window, cx),
            password_input: input_with("", window, cx),
            invite_input: input_with("settings.invite_code", window, cx),
            focus_topic_input: input_with("focus.topic", window, cx),
            personal_note_selected: None,
            shared_note_selected: None,
            focus_duration: 1500,
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

    fn set_language(&mut self, language: &str, cx: &mut Context<Self>) {
        let mut settings = self.core.settings_snapshot();
        settings.language = language.to_string();
        self.core.save_settings(settings);
        *self
            .core
            .language
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = language.to_string();
        cx.notify();
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

    fn section_header(&self, title: &str, cx: &Context<Self>) -> AnyElement {
        div()
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().colors.muted_foreground)
            .child(title.to_string())
            .into_any_element()
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
        let status = self.core.status_text();
        let presence = self
            .core
            .engine
            .state
            .read(|s| s.partner_presence.clone())
            .flatten();
        let partner_name = self
            .core
            .engine
            .state
            .read(|s| s.partner.as_ref().map(|p| p.display_name.clone()))
            .flatten()
            .unwrap_or_else(|| self.t("settings.no_partner").to_string());
        let online = presence
            .as_ref()
            .is_some_and(|p| p.online && p.state != PresenceState::Dnd);
        let input = self.chat_input.clone();
        let view = cx.entity().downgrade();
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(8.)).rounded_full().bg(if online {
                                rgb(0x66bb6a)
                            } else {
                                rgb(0xbbbbbb)
                            }))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(partner_name),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.colors.muted_foreground)
                            .child(status),
                    ),
            )
            .child(
                div()
                    .id("chat-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(messages.into_iter().map(|message| {
                        let mine = my_id.as_ref().is_some_and(|id| message.sender_id == *id);
                        div().flex().when(mine, |row| row.justify_end()).child(
                            div()
                                .max_w(px(280.))
                                .px_2()
                                .py_1()
                                .rounded(px(8.))
                                .bg(if mine {
                                    theme.colors.primary
                                } else {
                                    theme.colors.secondary
                                })
                                .text_color(if mine {
                                    theme.colors.primary_foreground
                                } else {
                                    theme.colors.foreground
                                })
                                .child(div().text_size(px(13.)).child(message.body.clone())),
                        )
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&input)))
                    .child(
                        Button::new("chat-send")
                            .label(self.t("chat.send"))
                            .on_click({
                                let view = view.clone();
                                move |_, _window, cx| {
                                    let _ = view.update_in(cx, |view, window, cx| {
                                        view.send_chat(window, cx)
                                    });
                                }
                            }),
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
        let zh = self.language() == "zh-CN";
        let input = self.letter_input.clone();
        let view = cx.entity().downgrade();
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(div().child(Input::new(&input)))
            .child(
                Button::new("letter-send")
                    .label(self.t("letters.leave"))
                    .on_click({
                        let view = view.clone();
                        move |_, _window, cx| {
                            let _ =
                                view.update_in(cx, |view, window, cx| view.send_letter(window, cx));
                        }
                    }),
            )
            .child(
                div()
                    .id("letter-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(letters.into_iter().map(|letter| {
                        let unread = my_id
                            .as_ref()
                            .is_some_and(|id| letter.sender_id != *id && letter.read_at.is_none());
                        let view = view.clone();
                        let letter_id = letter.id.clone();
                        let view_read = view.clone();
                        v_flex()
                            .p_2()
                            .rounded(px(8.))
                            .bg(if unread {
                                theme.colors.secondary
                            } else {
                                theme.colors.background
                            })
                            .border_1()
                            .border_color(theme.colors.border)
                            .child(div().text_size(px(13.)).child(letter.body.clone()))
                            .when(unread, |card| {
                                card.child(
                                    Button::new(SharedString::from(format!("read-{}", letter_id)))
                                        .label(if zh { "标记已读" } else { "Mark read" })
                                        .on_click(move |_, _, cx| {
                                            let id = letter_id.clone();
                                            let _ = view_read.update_in(cx, |v, _window, _cx| {
                                                v.core.engine.command(SyncCommand::Enqueue(
                                                    SyncOp::MarkLetterRead { letter_id: id },
                                                ));
                                            });
                                        }),
                                )
                            })
                    })),
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
        let zh = self.language() == "zh-CN";
        let mut panel = v_flex().flex_1().gap_2();
        if let Some(session) = focus {
            let remaining = session.remaining_ms(now_ms()).unwrap_or(0);
            let minutes = remaining / 60_000;
            let seconds = (remaining % 60_000) / 1000;
            let initiator = me_id.as_ref().is_some_and(|id| id == &session.initiator_id);
            panel = panel
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{minutes:02}:{seconds:02}")),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(theme.colors.muted_foreground)
                        .child(session.topic.clone().unwrap_or_default()),
                );
            match session.state {
                FocusState::Invited => {
                    if !initiator {
                        panel = panel
                            .child(div().text_size(px(12.)).child(self.t("focus.waiting")))
                            .child(
                                Button::new("focus-accept")
                                    .label(if zh { "接受" } else { "Accept" })
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
                                    .label(if zh { "婉拒" } else { "Decline" })
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
                        panel = panel.child(
                            Button::new("focus-cancel")
                                .label(self.t("focus.cancel"))
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
                    panel = panel.child(
                        div()
                            .flex()
                            .gap_2()
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
                            ),
                    );
                }
                FocusState::Paused => {
                    panel = panel.child(
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
        } else {
            let topic_input = self.focus_topic_input.clone();
            let durations = [25i64, 45, 50, 90];
            panel = panel
                .child(self.section_header(self.t("focus.invite"), cx))
                .child(div().child(Input::new(&topic_input)))
                .child(div().flex().gap_2().children(durations.map(|mins| {
                    let view = view.clone();
                    Button::new(SharedString::from(format!("dur-{mins}")))
                        .label(format!("{} {}", mins, self.t("focus.minutes")))
                        .on_click(move |_, _, cx| {
                            let _ = view.update_in(cx, |v, _window, _cx| {
                                v.focus_duration = mins * 60;
                            });
                        })
                })))
                .child(
                    Button::new("focus-invite")
                        .label(self.t("focus.invite"))
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                let _ = view.update_in(cx, |view, window, cx| {
                                    view.invite_focus(window, cx)
                                });
                            }
                        }),
                );
        }
        panel.into_any_element()
    }

    fn render_todos(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let personal = self.core.todos_store.load();
        let shared = self
            .core
            .engine
            .state
            .read(|s| s.shared_todos.clone())
            .unwrap_or_default();
        let input = self.todo_input.clone();
        let view = cx.entity().downgrade();
        let zh = self.language() == "zh-CN";
        let mut list = v_flex().gap_1();
        list = list.child(self.section_header(self.t("todos.personal"), cx));
        for todo in personal.todos.iter().take(30) {
            let id = todo.id.clone();
            let id_toggle = id.clone();
            let id_delete = id.clone();
            let done = todo.completed_at.is_some();
            let view_click = view.clone();
            let view_delete = view.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(if done {
                                theme.colors.muted_foreground
                            } else {
                                theme.colors.foreground
                            })
                            .child(if done {
                                format!("✓ {}", todo.title)
                            } else {
                                todo.title.clone()
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("todo-toggle-{id_toggle}")))
                            .label(if done { "↩" } else { "✓" })
                            .on_click(move |_, _window, cx| {
                                let _ = view_click.update_in(cx, |v, window, cx| {
                                    v.toggle_personal_todo(window, &id_toggle, cx)
                                });
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("todo-del-{id_delete}")))
                            .label("✕")
                            .on_click(move |_, _window, cx| {
                                let _ = view_delete.update_in(cx, |v, window, cx| {
                                    v.delete_personal_todo(window, &id_delete, cx)
                                });
                            }),
                    ),
            );
        }
        list = list.child(self.section_header(self.t("todos.shared"), cx));
        for todo in shared.iter().take(30) {
            let id = todo.id.clone();
            let _id_toggle = id.clone();
            let done = todo.completed_at.is_some();
            let view_click = view.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(if done {
                                theme.colors.muted_foreground
                            } else {
                                theme.colors.foreground
                            })
                            .child(if done {
                                format!("✓ {}", todo.title)
                            } else {
                                todo.title.clone()
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("shared-toggle-{}", id)))
                            .label(if done { "↩" } else { "✓" })
                            .on_click(move |_, _window, cx| {
                                let id = id.clone();
                                let _ = view_click.update_in(cx, |v, window, cx| {
                                    v.toggle_shared_todo(window, id, done, cx)
                                });
                            }),
                    ),
            );
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(div().child(Input::new(&input)))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("todo-add-personal")
                            .label(self.t("todos.add"))
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
                            .label(if zh {
                                "添加共享待办"
                            } else {
                                "Add shared"
                            })
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |view, window, cx| {
                                        view.add_shared_todo(window, cx)
                                    });
                                }
                            }),
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
        let _theme = cx.theme().clone();
        let personal = self.core.notes_store.load();
        let shared = self
            .core
            .engine
            .state
            .read(|s| s.shared_notes.clone())
            .unwrap_or_default();
        let input = self.note_input.clone();
        let view = cx.entity().downgrade();
        let zh = self.language() == "zh-CN";
        let mut rows = v_flex().gap_1();
        rows = rows.child(self.section_header(self.t("notes.personal"), cx));
        for note in personal.notes.iter().take(20) {
            let id = note.id.clone();
            let view_select = view.clone();
            let selected = self.personal_note_selected.as_ref() == Some(&note.id);
            rows = rows.child(
                Button::new(SharedString::from(format!("pnote-{}", id)))
                    .label(note.content.chars().take(18).collect::<String>())
                    .when(selected, |button| button.outline())
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
                                v.note_input
                                    .update(cx, |state, cx| state.set_value(content, window, cx));
                            }
                        });
                    }),
            );
        }
        rows = rows.child(self.section_header(self.t("notes.shared"), cx));
        for note in shared.iter().take(20) {
            let id = note.id.clone();
            let view_select = view.clone();
            let selected = self.shared_note_selected.as_ref() == Some(&note.id);
            rows = rows.child(
                Button::new(SharedString::from(format!("snote-{}", id)))
                    .label(format!(
                        "{} (rev {})",
                        note.content.chars().take(14).collect::<String>(),
                        note.revision
                    ))
                    .when(selected, |button| button.outline())
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
                                v.note_input
                                    .update(cx, |state, cx| state.set_value(content, window, cx));
                            }
                        });
                    }),
            );
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("pnote-add")
                            .label(self.t("notes.add"))
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
                            .label(if zh { "保存个人" } else { "Save personal" })
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
                            .label(if zh { "新建共享" } else { "New shared" })
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
                            .label(if zh { "保存共享" } else { "Save shared" })
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |view, window, cx| {
                                        view.save_shared_note(window, cx)
                                    });
                                }
                            }),
                    ),
            )
            .child(div().child(Input::new(&input)))
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
        let zh = self.language() == "zh-CN";
        let mut rows = v_flex().gap_1();
        rows = rows.child(self.section_header(self.t("notes.personal"), cx));
        for item in personal.anniversaries.iter().take(20) {
            let days = clock::anniversary_days(&item.date, item.repeat_yearly, &now).unwrap_or(0);
            rows = rows.child(
                div()
                    .flex()
                    .justify_between()
                    .child(div().text_size(px(13.)).child(item.name.clone()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.colors.muted_foreground)
                            .child(format!(
                                "{} · {} {}",
                                item.date,
                                days.abs(),
                                self.t("days.days_left")
                            )),
                    ),
            );
        }
        rows = rows.child(self.section_header(self.t("notes.shared"), cx));
        for item in shared.iter().take(20) {
            let days = clock::anniversary_days(&item.date, item.repeat_yearly, &now).unwrap_or(0);
            rows = rows.child(
                div()
                    .flex()
                    .justify_between()
                    .child(div().text_size(px(13.)).child(item.name.clone()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.colors.muted_foreground)
                            .child(format!(
                                "{} · {} {}",
                                item.date,
                                days.abs(),
                                self.t("days.days_left")
                            )),
                    ),
            );
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&name_input)))
                    .child(div().w(px(96.)).child(Input::new(&date_input))),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("ann-add-personal")
                            .label(self.t("notes.personal"))
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
                            .label(self.t("notes.shared"))
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
            .when(zh, |panel| {
                panel.child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme.colors.muted_foreground)
                        .child("日期格式：2025-05-20"),
                )
            })
            .child(
                div()
                    .id("ann-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(rows),
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
        let zh = self.language() == "zh-CN";
        let mut rows = v_flex().gap_1();
        for alarm in alarms.alarms.iter().take(30) {
            let id = alarm.id.clone();
            let id_toggle = id.clone();
            let id_delete = id.clone();
            let view_toggle = view.clone();
            let view_delete = view.clone();
            let next = clock::next_trigger(alarm, &now)
                .map(|t| clock::format_time(&t, true))
                .unwrap_or_else(|| "—".to_string());
            rows = rows.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(13.))
                            .text_color(if alarm.enabled {
                                theme.colors.foreground
                            } else {
                                theme.colors.muted_foreground
                            })
                            .child(format!(
                                "{} {} · {} → {}",
                                if alarm.enabled { "🔔" } else { "🔕" },
                                alarm.title,
                                alarm.time,
                                next
                            )),
                    )
                    .child(
                        Button::new(SharedString::from(format!("alarm-toggle-{}", id_toggle)))
                            .label(if zh {
                                if alarm.enabled { "停用" } else { "启用" }
                            } else if alarm.enabled {
                                "Off"
                            } else {
                                "On"
                            })
                            .on_click(move |_, _window, cx| {
                                let _ = view_toggle.update_in(cx, |v, window, cx| {
                                    v.toggle_alarm(window, &id_toggle, cx)
                                });
                            }),
                    )
                    .child(
                        Button::new(SharedString::from(format!("alarm-del-{}", id_delete)))
                            .label("✕")
                            .on_click(move |_, _, cx| {
                                let id = id.clone();
                                let _ = view_delete
                                    .update_in(cx, |v, window, cx| v.delete_alarm(window, &id, cx));
                            }),
                    ),
            );
        }
        v_flex()
            .flex_1()
            .gap_2()
            .p_2()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&title_input)))
                    .child(div().w(px(88.)).child(Input::new(&time_input))),
            )
            .child(
                Button::new("alarm-add")
                    .label(self.t("alarms.add"))
                    .on_click(move |_, _, cx| {
                        let _ = view.update_in(cx, |view, window, cx| view.add_alarm(window, cx));
                    }),
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
        let relation = self
            .core
            .engine
            .state
            .read(|s| s.relation.clone())
            .flatten();
        let status = self.core.status_text();
        let zh = self.language() == "zh-CN";
        let server_input = self.server_input.clone();
        let username_input = self.username_input.clone();
        let password_input = self.password_input.clone();
        let invite_input = self.invite_input.clone();
        let view = cx.entity().downgrade();
        let _core = Arc::clone(&self.core);

        let mut panel = v_flex().flex_1().gap_2();
        panel = panel
            .child(self.section_header(self.t("settings.account"), cx))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.colors.muted_foreground)
                    .child(if account.username.is_empty() {
                        status
                    } else {
                        format!("{} · {}", account.display_name, status)
                    }),
            );
        if account.token.is_empty() {
            panel = panel
                .child(div().child(Input::new(&server_input)))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&username_input)))
                        .child(div().flex_1().child(Input::new(&password_input))),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("register")
                                .label(self.t("settings.register"))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Register, cx)
                                        });
                                    }
                                }),
                        )
                        .child(
                            Button::new("login")
                                .label(self.t("settings.login"))
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Login, cx)
                                        });
                                    }
                                }),
                        ),
                );
        } else {
            panel = panel
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .flex_wrap()
                        .child(
                            Button::new("invite-create")
                                .label(self.t("settings.invite_code"))
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
                                .on_click({
                                    let view = view.clone();
                                    move |_, _, cx| {
                                        let _ = view.update_in(cx, |v, _window, cx| {
                                            v.spawn_account_op(AccountOp::Logout, cx)
                                        });
                                    }
                                }),
                        ),
                )
                .child(div().child(Input::new(&invite_input)))
                .child(
                    Button::new("invite-accept")
                        .label(self.t("settings.accept_invite"))
                        .on_click({
                            let view = view.clone();
                            move |_, _, cx| {
                                let _ = view.update_in(cx, |v, _window, cx| {
                                    v.spawn_account_op(AccountOp::AcceptInvite, cx)
                                });
                            }
                        }),
                );
        }
        panel = panel
            .child(self.section_header(self.t("settings.partner"), cx))
            .child(div().text_size(px(12.)).child(match (&relation, &partner) {
                (Some(_), Some(partner)) => partner.display_name.clone(),
                _ => self.t("settings.no_partner").to_string(),
            }))
            .child(self.section_header(self.t("settings.language"), cx))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("poke-here")
                            .label(self.t("tray.poke"))
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |view, window, cx| {
                                        view.set_poke(window, cx)
                                    });
                                }
                            }),
                    )
                    .child(
                        Button::new("lang-switch")
                            .label(if zh { "English" } else { "中文" })
                            .on_click({
                                let view = view.clone();
                                move |_, _, cx| {
                                    let _ = view.update_in(cx, |v, _, cx| {
                                        let language = if zh { "en" } else { "zh-CN" };
                                        v.set_language(language, cx);
                                    });
                                    cx.refresh_windows();
                                }
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme.colors.muted_foreground)
                    .child(if zh {
                        "个人待办、个人便签与闹钟只保存在本地，不会同步给伴侣"
                    } else {
                        "Personal todos, notes and alarms stay on this device"
                    }),
            );
        panel.into_any_element()
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
        let language = self.language();
        let view = cx.entity().downgrade();
        let mut bar = TabBar::new("hub-tabs")
            .selected_index(self.tab.index())
            .on_click(move |index: &usize, _, cx| {
                let index = *index;
                let _ = view.update(cx, |view, cx| {
                    if let Some(tab) = TABS.get(index) {
                        view.tab = *tab;
                        cx.notify();
                    }
                });
            });
        for tab in TABS {
            bar = bar.child(GpuiTab::new().label(tab.title(&language)));
        }
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
            .bg(rgb(0xfff8f0))
            .child(bar)
            .child(content)
    }
}
