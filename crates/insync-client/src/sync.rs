//! The sync engine: one dedicated OS thread owning a tokio current-thread
//! runtime, the REST client, the WebSocket loop, the outbox and the state
//! mirror. UI threads talk to it through [`SyncCommand`]s and read
//! [`SharedState`]; they never block on the network.

use crate::http::{InSyncClient, NoteConflictError};
use crate::state::{EngineEvent, EventSender, SharedState, SyncStatus};
use insync_core::Outbox;
use insync_core::sync::{EventDeduper, MessageCache, reconnect_delay_ms};
use insync_core::{EventData, EventKind, SyncOp, UnixMs};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Commands the UI sends to the engine.
#[derive(Debug, Clone)]
pub enum SyncCommand {
    /// Queue an operation (local-first: the UI already applied it locally).
    Enqueue(SyncOp),
    /// (Re)connect using the current token; starts the realtime loop.
    Connect,
    /// Drop the connection and stop retrying until the next Connect.
    Disconnect,
    /// Refresh overview + shared collections now.
    Refresh,
}

/// How the engine obtains the current bearer token (the app persists it).
pub type TokenSource = Arc<dyn Fn() -> Option<String> + Send + Sync>;

pub struct EngineConfig {
    pub server_url: String,
    pub token_source: TokenSource,
}

/// The engine handle the application keeps.
pub struct EngineHandle {
    commands: std::sync::mpsc::Sender<SyncCommand>,
    events: Mutex<Receiver<EngineEvent>>,
    pub state: Arc<SharedState>,
    stop: Arc<std::sync::atomic::AtomicBool>,
}

impl EngineHandle {
    /// Spawns the engine thread with its own tokio runtime.
    pub fn spawn(config: EngineConfig) -> Self {
        let (command_tx, command_rx) = std::sync::mpsc::channel();
        let (event_tx, event_rx) = std::sync::mpsc::sync_channel::<EngineEvent>(256);
        let state = Arc::new(SharedState::default());
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_state = Arc::clone(&state);
        let thread_stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("insync-sync".to_string())
            .spawn(move || {
                let engine = Engine::new(config, thread_state, event_tx);
                engine.run(command_rx, thread_stop);
            })
            .expect("spawn sync engine thread");
        Self {
            commands: command_tx,
            events: Mutex::new(event_rx),
            state,
            stop,
        }
    }

    pub fn command(&self, command: SyncCommand) {
        let _ = self.commands.send(command);
    }

    /// Non-blocking event poll for the UI loop.
    pub fn try_recv_event(&self) -> Option<EngineEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .try_recv()
            .ok()
    }

    /// Blocks up to `timeout` for the next event (used by tests).
    pub fn recv_event_timeout(&self, timeout: Duration) -> Option<EngineEvent> {
        self.events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv_timeout(timeout)
            .ok()
    }

    pub fn shutdown(self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        // unblock the command wait
        let _ = self.commands.send(SyncCommand::Refresh);
    }
}

/// Frames arriving from the WebSocket loop task.
enum WsDelivery {
    Connected,
    Frame(Box<insync_core::ServerFrame>),
    Failed,
}

enum OpOutcome {
    Done,
    Drop(&'static str),
    Retry,
}

struct Engine {
    config: EngineConfig,
    state: Arc<SharedState>,
    events: std::sync::mpsc::SyncSender<EngineEvent>,
    client: InSyncClient,
    outbox: Outbox,
    message_cache: MessageCache,
    deduper: EventDeduper,
    cursor: u64,
    last_poke_sent_at: Option<UnixMs>,
    ws_event_rx: Option<Receiver<WsDelivery>>,
    reconnect_attempt: u32,
    next_reconnect_at: Option<Instant>,
    my_read_seq: u64,
    me_id: Option<String>,
}

impl Engine {
    fn new(config: EngineConfig, state: Arc<SharedState>, events: EventSender) -> Self {
        let client = InSyncClient::new(&config.server_url);
        Self {
            config,
            state,
            events,
            client,
            outbox: Outbox::new(256),
            message_cache: MessageCache::new(500),
            deduper: EventDeduper::new(512),
            cursor: 0,
            last_poke_sent_at: None,
            ws_event_rx: None,
            reconnect_attempt: 0,
            next_reconnect_at: None,
            my_read_seq: 0,
            me_id: None,
        }
    }

    fn emit(&self, event: EngineEvent) {
        let _ = self.events.try_send(event);
    }

    fn run(mut self, commands: Receiver<SyncCommand>, stop: Arc<std::sync::atomic::AtomicBool>) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio current-thread runtime");
        let mut last_refresh: Option<Instant> = None;
        loop {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            // 1. commands
            match commands.recv_timeout(Duration::from_millis(120)) {
                Ok(command) => self.handle_command(&runtime, command),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
            while let Ok(command) = commands.try_recv() {
                self.handle_command(&runtime, command);
            }

            // 2. realtime frames
            let ws_frames = self.ws_event_rx.take();
            if let Some(receiver) = ws_frames {
                let mut live = true;
                while let Ok(delivery) = receiver.try_recv() {
                    match delivery {
                        WsDelivery::Connected => {
                            self.state.set_status(SyncStatus::Online);
                            self.emit(EngineEvent::StatusChanged(SyncStatus::Online));
                        }
                        WsDelivery::Frame(frame) => {
                            self.handle_server_frame(&runtime, *frame);
                        }
                        WsDelivery::Failed => {
                            live = false;
                        }
                    }
                }
                if live {
                    self.ws_event_rx = Some(receiver);
                } else {
                    self.schedule_retry(None);
                }
            }

            // 3. reconnect scheduling
            if self.ws_event_rx.is_none()
                && self.state.status() != SyncStatus::Offline
                && self.state.status() != SyncStatus::NeedsRelogin
                && self
                    .next_reconnect_at
                    .is_some_and(|at| Instant::now() >= at)
            {
                self.connect(&runtime);
            }

            // 4. outbox (only makes sense while the session is alive)
            if self.ws_event_rx.is_some() {
                self.drain_outbox(&runtime);
            }

            // 5. periodic refresh while online
            if self.ws_event_rx.is_some()
                && last_refresh.is_none_or(|at| at.elapsed() > Duration::from_secs(30))
            {
                self.refresh_all(&runtime);
                last_refresh = Some(Instant::now());
            }
        }
    }

    fn handle_command(&mut self, runtime: &tokio::runtime::Runtime, command: SyncCommand) {
        match command {
            SyncCommand::Connect => {
                self.state.set_status(SyncStatus::Connecting);
                self.reconnect_attempt = 0;
                self.next_reconnect_at = Some(Instant::now());
                let _ = runtime;
            }
            SyncCommand::Disconnect => {
                self.ws_event_rx = None;
                self.next_reconnect_at = None;
                self.state.set_status(SyncStatus::Offline);
            }
            SyncCommand::Refresh => {
                if self.ws_event_rx.is_some() {
                    self.refresh_all(runtime);
                }
            }
            SyncCommand::Enqueue(op) => {
                if matches!(
                    self.outbox.enqueue(op, now_ms(), self.last_poke_sent_at),
                    insync_core::outbox::EnqueueOutcome::Rejected("poke_cooldown")
                ) {
                    self.emit(EngineEvent::PokeFailed);
                }
            }
        }
    }

    /// Establishes the REST session (overview) then the WS loop.
    fn connect(&mut self, runtime: &tokio::runtime::Runtime) {
        if (self.config.token_source)().is_none() {
            self.state.set_status(SyncStatus::Offline);
            return;
        }
        self.refresh_client_token();
        match runtime.block_on(self.client.overview()) {
            Ok(overview) => {
                self.my_read_seq = overview.my_read_seq;
                self.me_id = Some(overview.user.id.clone());
                self.reconnect_attempt = 0;
                self.next_reconnect_at = None;
                self.start_ws();
                self.refresh_all(runtime);
            }
            Err(error) => self.schedule_retry(Some(error)),
        }
    }

    fn refresh_client_token(&mut self) {
        if let Some(token) = (self.config.token_source)() {
            self.client = InSyncClient::new(&self.config.server_url).with_token(token);
        }
    }

    fn schedule_retry(&mut self, failure: Option<crate::error::ClientError>) {
        if let Some(crate::error::ClientError::TokenExpired)
        | Some(crate::error::ClientError::Unauthorized(_)) = failure
        {
            self.ws_event_rx = None;
            self.state.set_status(SyncStatus::NeedsRelogin);
            self.emit(EngineEvent::StatusChanged(SyncStatus::NeedsRelogin));
            return;
        }
        let delay = reconnect_delay_ms(self.reconnect_attempt);
        self.reconnect_attempt = self.reconnect_attempt.saturating_add(1);
        self.next_reconnect_at = Some(Instant::now() + Duration::from_millis(delay as u64));
        self.state
            .set_status(SyncStatus::Retrying(self.reconnect_attempt));
        self.emit(EngineEvent::StatusChanged(SyncStatus::Retrying(
            self.reconnect_attempt,
        )));
    }

    /// Spawns the realtime loop on its own thread (with its own tokio
    /// runtime); frames land in a std channel the engine loop polls. The
    /// thread exits when the engine drops the receiver or the socket dies.
    fn start_ws(&mut self) {
        let client = self.client.clone();
        let cursor = self.cursor;
        let (tx, rx) = std::sync::mpsc::channel::<WsDelivery>();
        let _ = self
            .events
            .try_send(EngineEvent::StatusChanged(SyncStatus::Connecting));
        std::thread::Builder::new()
            .name("insync-ws".to_string())
            .spawn(move || {
                ws_loop(client, cursor, tx);
            })
            .expect("spawn ws thread");
        self.ws_event_rx = Some(rx);
    }

    fn handle_server_frame(
        &mut self,
        runtime: &tokio::runtime::Runtime,
        frame: insync_core::ServerFrame,
    ) {
        match frame {
            insync_core::ServerFrame::Ready { latest_seq } => {
                if latest_seq > self.cursor {
                    self.catch_up_events(runtime, self.cursor);
                }
            }
            insync_core::ServerFrame::Event(event) => {
                self.apply_event(*event);
            }
            insync_core::ServerFrame::Error { code, .. } => {
                if code == "EVENTS_LAGGED" {
                    self.catch_up_events(runtime, self.cursor);
                } else if code == insync_core::api::error_codes::RELATION_INACTIVE {
                    self.ws_event_rx = None;
                    self.state.set_status(SyncStatus::NeedsRelogin);
                }
            }
            insync_core::ServerFrame::Pong => {}
        }
    }

    /// Pulls events after `from` over REST until caught up.
    fn catch_up_events(&mut self, runtime: &tokio::runtime::Runtime, from: u64) {
        let mut cursor = from;
        loop {
            match runtime.block_on(self.client.events(cursor)) {
                Ok((events, latest)) => {
                    if events.is_empty() {
                        self.cursor = self.cursor.max(latest);
                        return;
                    }
                    for event in events {
                        let seq = event.seq;
                        self.apply_event(event);
                        cursor = cursor.max(seq);
                    }
                    if cursor >= latest {
                        self.cursor = self.cursor.max(latest);
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    }

    /// Applies one event to the state mirror (idempotent by event id).
    fn apply_event(&mut self, event: insync_core::EventFrame) {
        if self.deduper.is_duplicate(&event.id) {
            return;
        }
        self.cursor = self.cursor.max(event.seq);
        let my_id = self.me_id.clone().unwrap_or_default();
        match (&event.kind, &event.data) {
            (EventKind::MessageCreated, EventData::Message(message)) => {
                self.message_cache.merge(vec![message.clone()]);
                if message.sender_id != my_id && message.seq > self.my_read_seq {
                    let unread = self
                        .message_cache
                        .unread_from_partner(&my_id, self.my_read_seq);
                    self.state.set_unread_messages(unread);
                }
                self.state.update(|snapshot| {
                    if !snapshot.messages.iter().any(|m| m.seq == message.seq) {
                        snapshot.messages.push(message.clone());
                        snapshot.messages.sort_by_key(|m| m.seq);
                    }
                });
            }
            (EventKind::LetterCreated, EventData::Letter(letter)) => {
                let letter = letter.clone();
                self.state.update(|snapshot| {
                    if !snapshot.letters.iter().any(|l| l.id == letter.id) {
                        snapshot.letters.insert(0, letter);
                    }
                });
            }
            (EventKind::LetterRead, EventData::LetterId { letter_id }) => {
                let letter_id = letter_id.clone();
                self.state.update(|snapshot| {
                    for letter in snapshot.letters.iter_mut() {
                        if letter.id == letter_id {
                            letter.read_at = Some(event.server_time);
                        }
                    }
                });
            }
            (EventKind::PresenceUpdated, EventData::Presence(presence)) => {
                let presence = presence.as_ref().clone();
                self.state.update(|snapshot| {
                    snapshot.partner_presence = Some(presence);
                });
            }
            (EventKind::RelationDissolved, _) => {
                self.outbox.drop_relation_ops();
                self.state.update(|snapshot| {
                    snapshot.relation = None;
                    snapshot.partner = None;
                    snapshot.partner_presence = None;
                    snapshot.active_focus = None;
                    snapshot.shared_todos.clear();
                    snapshot.shared_notes.clear();
                    snapshot.shared_anniversaries.clear();
                    snapshot.letters.clear();
                });
            }
            (EventKind::TodoCreated, EventData::Todo(todo))
            | (EventKind::TodoUpdated, EventData::Todo(todo))
            | (EventKind::TodoCompleted, EventData::Todo(todo)) => {
                let todo = todo.as_ref().clone();
                self.state.update(|snapshot| {
                    if let Some(slot) = snapshot.shared_todos.iter_mut().find(|t| t.id == todo.id) {
                        *slot = todo.clone();
                    } else {
                        snapshot.shared_todos.insert(0, todo);
                    }
                });
            }
            (EventKind::TodoDeleted, EventData::TodoId { todo_id }) => {
                let todo_id = todo_id.clone();
                self.state.update(|snapshot| {
                    snapshot.shared_todos.retain(|t| t.id != todo_id);
                });
            }
            (EventKind::NoteCreated, EventData::Note(note))
            | (EventKind::NoteUpdated, EventData::Note(note)) => {
                let note = note.as_ref().clone();
                self.state.update(|snapshot| {
                    if let Some(slot) = snapshot.shared_notes.iter_mut().find(|n| n.id == note.id) {
                        *slot = note.clone();
                    } else {
                        snapshot.shared_notes.insert(0, note);
                    }
                });
            }
            (EventKind::NoteDeleted, EventData::NoteId { note_id }) => {
                let note_id = note_id.clone();
                self.state.update(|snapshot| {
                    snapshot.shared_notes.retain(|n| n.id != note_id);
                });
            }
            (EventKind::AnniversaryCreated, EventData::Anniversary(item))
            | (EventKind::AnniversaryUpdated, EventData::Anniversary(item)) => {
                let item = item.as_ref().clone();
                self.state.update(|snapshot| {
                    if let Some(slot) = snapshot
                        .shared_anniversaries
                        .iter_mut()
                        .find(|a| a.id == item.id)
                    {
                        *slot = item.clone();
                    } else {
                        snapshot.shared_anniversaries.push(item);
                    }
                });
            }
            (EventKind::AnniversaryDeleted, EventData::AnniversaryId { anniversary_id }) => {
                let anniversary_id = anniversary_id.clone();
                self.state.update(|snapshot| {
                    snapshot
                        .shared_anniversaries
                        .retain(|a| a.id != anniversary_id);
                });
            }
            (
                EventKind::FocusInvited
                | EventKind::FocusAccepted
                | EventKind::FocusDeclined
                | EventKind::FocusCancelled
                | EventKind::FocusStarted
                | EventKind::FocusPaused
                | EventKind::FocusResumed
                | EventKind::FocusCompleted,
                EventData::Focus(session),
            ) => {
                let session = session.as_ref().clone();
                self.state.update(|snapshot| {
                    if session.state.is_terminal() {
                        if snapshot
                            .active_focus
                            .as_ref()
                            .is_some_and(|active| active.id == session.id)
                        {
                            snapshot.active_focus = None;
                        }
                    } else {
                        snapshot.active_focus = Some(session);
                    }
                });
            }
            _ => {}
        }
        self.emit(EngineEvent::Event(Arc::new(event)));
    }

    fn refresh_all(&mut self, runtime: &tokio::runtime::Runtime) {
        self.refresh_client_token();
        match runtime.block_on(self.client.overview()) {
            Ok(overview) => {
                self.my_read_seq = overview.my_read_seq;
                self.me_id = Some(overview.user.id.clone());
                let last_message = overview.last_message.clone();
                self.state.update(|snapshot| {
                    snapshot.me = Some(overview.user.clone());
                    snapshot.relation = overview.relation.clone();
                    snapshot.partner = overview.partner.clone();
                    snapshot.partner_presence = overview.partner_presence.clone();
                    snapshot.active_focus = overview.active_focus.clone();
                    snapshot.overview = Some(overview.clone());
                    if let Some(last) = last_message
                        && !snapshot.messages.iter().any(|m| m.seq == last.seq)
                    {
                        snapshot.messages.push(last);
                        snapshot.messages.sort_by_key(|m| m.seq);
                    }
                });
                if let Ok(page) =
                    runtime.block_on(self.client.messages(self.message_cache.highest_seq()))
                {
                    self.message_cache.merge(page.messages);
                    let unread = self
                        .message_cache
                        .unread_from_partner(self.me_id.as_deref().unwrap_or(""), self.my_read_seq);
                    self.state.set_unread_messages(unread);
                    self.state.update(|snapshot| {
                        snapshot.messages = self.message_cache.messages().to_vec();
                    });
                }
                self.emit(EngineEvent::OverviewRefreshed);
            }
            Err(error) => self.schedule_retry(Some(error)),
        }
        let todos = runtime.block_on(self.client.todos());
        let notes = runtime.block_on(self.client.notes());
        let anniversaries = runtime.block_on(self.client.anniversaries());
        let letters = runtime.block_on(self.client.letters());
        self.state.update(|snapshot| {
            if let Ok(todos) = todos {
                snapshot.shared_todos = todos;
            }
            if let Ok(notes) = notes {
                snapshot.shared_notes = notes;
            }
            if let Ok(anniversaries) = anniversaries {
                snapshot.shared_anniversaries = anniversaries;
            }
            if let Ok(letters) = letters {
                snapshot.letters = letters;
            }
        });
    }

    /// Drains the outbox: one op at a time, with the core backoff table.
    fn drain_outbox(&mut self, runtime: &tokio::runtime::Runtime) {
        let now = now_ms();
        let Some(drained) = self.outbox.take_ready(now) else {
            return;
        };
        let op = drained.entry.op.clone();
        let attempts = drained.entry.attempts;
        let outcome = self.execute_op(runtime, &op);
        match outcome {
            OpOutcome::Done => {
                self.outbox
                    .complete(&op, insync_core::outbox::CompleteOutcome::Done, now);
            }
            OpOutcome::Drop(reason) => {
                self.outbox.complete(
                    &op,
                    insync_core::outbox::CompleteOutcome::Dropped(reason),
                    now,
                );
                self.emit(EngineEvent::OpFailed {
                    op: op.describe().to_string(),
                    error: reason.to_string(),
                });
            }
            OpOutcome::Retry => {
                let after = insync_core::outbox::Outbox::backoff_ms(attempts);
                self.outbox.complete(
                    &op,
                    insync_core::outbox::CompleteOutcome::Retry { after_ms: after },
                    now,
                );
            }
        }
    }

    fn execute_op(&mut self, runtime: &tokio::runtime::Runtime, op: &SyncOp) -> OpOutcome {
        match op {
            SyncOp::SendMessage { client_id, body } => {
                let client_id = client_id.clone();
                let body = body.clone();
                match runtime.block_on(self.client.send_message(&client_id, &body)) {
                    Ok(message) => {
                        self.message_cache.merge(vec![message.clone()]);
                        self.state.update(|snapshot| {
                            if !snapshot.messages.iter().any(|m| m.seq == message.seq) {
                                snapshot.messages.push(message.clone());
                                snapshot.messages.sort_by_key(|m| m.seq);
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::MarkRead { last_message_seq } => {
                self.my_read_seq = (*last_message_seq).max(self.my_read_seq);
                match runtime.block_on(self.client.mark_read(*last_message_seq)) {
                    Ok(()) => {
                        let unread = self.message_cache.unread_from_partner(
                            self.me_id.as_deref().unwrap_or(""),
                            self.my_read_seq,
                        );
                        self.state.set_unread_messages(unread);
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::SendPoke { client_id } => {
                self.last_poke_sent_at = Some(now_ms());
                let client_id = client_id.clone();
                match runtime.block_on(self.client.send_poke(&client_id)) {
                    Ok(Some(_)) => OpOutcome::Done,
                    Ok(None) => OpOutcome::Drop("poke rate limited"),
                    Err(error) => classify(error),
                }
            }
            SyncOp::SendLetter { client_id, body } => {
                let client_id = client_id.clone();
                let body = body.clone();
                match runtime.block_on(self.client.send_letter(&client_id, &body)) {
                    Ok(letter) => {
                        self.state.update(|snapshot| {
                            if !snapshot.letters.iter().any(|l| l.id == letter.id) {
                                snapshot.letters.insert(0, letter);
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::MarkLetterRead { letter_id } => {
                let letter_id = letter_id.clone();
                match runtime.block_on(self.client.read_letter(&letter_id)) {
                    Ok(()) => {
                        self.state.update(|snapshot| {
                            for letter in snapshot.letters.iter_mut() {
                                if letter.id == letter_id {
                                    letter.read_at = Some(now_ms());
                                }
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::FocusAction {
                focus_id, action, ..
            } => {
                let name = match action {
                    insync_core::FocusAction::Accept => "accept",
                    insync_core::FocusAction::Decline => "decline",
                    insync_core::FocusAction::Cancel => "cancel",
                    insync_core::FocusAction::Pause => "pause",
                    insync_core::FocusAction::Resume => "resume",
                    insync_core::FocusAction::FinishEarly => "finish-early",
                };
                let focus_id = focus_id.clone();
                match runtime.block_on(self.client.focus_action(&focus_id, name)) {
                    Ok(session) => {
                        self.state.update(|snapshot| {
                            if session.state.is_terminal() {
                                snapshot.active_focus = None;
                            } else {
                                snapshot.active_focus = Some(session);
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(crate::error::ClientError::Conflict { .. }) => {
                        // a competing transition won; converge on server state
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::FocusInvite {
                client_id,
                topic,
                duration_secs,
            } => {
                match runtime.block_on(self.client.focus_invite(
                    client_id,
                    topic.clone(),
                    *duration_secs,
                )) {
                    Ok(session) => {
                        self.state.update(|snapshot| {
                            snapshot.active_focus = Some(session);
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::CreateTodo {
                client_id,
                title,
                detail,
                priority,
                due_at,
            } => {
                match runtime.block_on(self.client.create_todo(
                    &insync_core::api::CreateTodoRequest {
                        client_id: client_id.clone(),
                        title: title.clone(),
                        detail: detail.clone(),
                        priority: *priority,
                        due_at: *due_at,
                    },
                )) {
                    Ok(todo) => {
                        self.state.update(|snapshot| {
                            if !snapshot.shared_todos.iter().any(|t| t.id == todo.id) {
                                snapshot.shared_todos.insert(0, todo);
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::UpdateTodo { todo_id, req } => {
                let todo_id = todo_id.clone();
                let req = req.clone();
                match runtime.block_on(self.client.update_todo(&todo_id, &req)) {
                    Ok(todo) => {
                        self.state.update(|snapshot| {
                            if let Some(slot) =
                                snapshot.shared_todos.iter_mut().find(|t| t.id == todo_id)
                            {
                                *slot = todo;
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::DeleteTodo { todo_id } => {
                let todo_id = todo_id.clone();
                match runtime.block_on(self.client.delete_todo(&todo_id)) {
                    Ok(()) => {
                        self.state.update(|snapshot| {
                            snapshot.shared_todos.retain(|t| t.id != todo_id);
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::CreateNote { client_id, content } => {
                match runtime.block_on(self.client.create_note(
                    &insync_core::api::CreateNoteRequest {
                        client_id: client_id.clone(),
                        content: content.clone(),
                    },
                )) {
                    Ok(note) => {
                        self.state.update(|snapshot| {
                            if !snapshot.shared_notes.iter().any(|n| n.id == note.id) {
                                snapshot.shared_notes.insert(0, note);
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::UpdateNote {
                note_id,
                content,
                base_revision,
            } => {
                let note_id = note_id.clone();
                let request = insync_core::api::UpdateNoteRequest {
                    content: content.clone(),
                    base_revision: *base_revision,
                };
                match runtime.block_on(self.client.update_note(&note_id, &request)) {
                    Ok(Ok(note)) => {
                        self.state.update(|snapshot| {
                            if let Some(slot) =
                                snapshot.shared_notes.iter_mut().find(|n| n.id == note_id)
                            {
                                *slot = note;
                            }
                        });
                        OpOutcome::Done
                    }
                    Ok(Err(NoteConflictError { current })) => {
                        self.state.update(|snapshot| {
                            if let Some(slot) = snapshot
                                .shared_notes
                                .iter_mut()
                                .find(|n| n.id == current.id)
                            {
                                *slot = current.clone();
                            }
                        });
                        self.emit(EngineEvent::NoteConflict {
                            note_id: note_id.clone(),
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::DeleteNote { note_id } => {
                let note_id = note_id.clone();
                match runtime.block_on(self.client.delete_note(&note_id)) {
                    Ok(()) => {
                        self.state.update(|snapshot| {
                            snapshot.shared_notes.retain(|n| n.id != note_id);
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::CreateAnniversary { client_id, req } => {
                let request = insync_core::api::CreateAnniversaryRequest {
                    client_id: client_id.clone(),
                    name: req.name.clone(),
                    date: req.date.clone(),
                    repeat_yearly: req.repeat_yearly,
                    note: req.note.clone(),
                };
                match runtime.block_on(self.client.create_anniversary(&request)) {
                    Ok(item) => {
                        self.state.update(|snapshot| {
                            if !snapshot
                                .shared_anniversaries
                                .iter()
                                .any(|a| a.id == item.id)
                            {
                                snapshot.shared_anniversaries.push(item);
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::UpdateAnniversary {
                anniversary_id,
                req,
            } => {
                let anniversary_id = anniversary_id.clone();
                let req = req.clone();
                match runtime.block_on(self.client.update_anniversary(&anniversary_id, &req)) {
                    Ok(item) => {
                        self.state.update(|snapshot| {
                            if let Some(slot) = snapshot
                                .shared_anniversaries
                                .iter_mut()
                                .find(|a| a.id == anniversary_id)
                            {
                                *slot = item;
                            }
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::DeleteAnniversary { anniversary_id } => {
                let anniversary_id = anniversary_id.clone();
                match runtime.block_on(self.client.delete_anniversary(&anniversary_id)) {
                    Ok(()) => {
                        self.state.update(|snapshot| {
                            snapshot
                                .shared_anniversaries
                                .retain(|a| a.id != anniversary_id);
                        });
                        OpOutcome::Done
                    }
                    Err(error) => classify(error),
                }
            }
            SyncOp::SetPresence { state } => {
                match runtime.block_on(self.client.set_presence(*state)) {
                    Ok(()) => OpOutcome::Done,
                    Err(error) => classify(error),
                }
            }
        }
    }
}

/// Runs the realtime loop on the dedicated ws thread.
fn ws_loop(client: InSyncClient, cursor: u64, tx: std::sync::mpsc::Sender<WsDelivery>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime for ws");
    runtime.block_on(async move {
        let token = client.token().unwrap_or_default().to_string();
        let request = tokio_tungstenite::tungstenite::client::IntoClientRequest::into_client_request(
            ws_url(client.base(), &token),
        );
        let Ok(request) = request else {
            let _ = tx.send(WsDelivery::Failed);
            return;
        };
        let connect = tokio_tungstenite::connect_async(request).await;
        let Ok((mut ws, _)) = connect else {
            let _ = tx.send(WsDelivery::Failed);
            return;
        };
        let _ = tx.send(WsDelivery::Connected);
        use futures_util::{SinkExt, StreamExt};
        let hello = serde_json::to_string(&insync_core::ClientFrame::Hello {
            last_seq: Some(cursor),
        })
        .unwrap_or_default();
        if ws
            .send(tokio_tungstenite::tungstenite::Message::Text(hello.into()))
            .await
            .is_err()
        {
            let _ = tx.send(WsDelivery::Failed);
            return;
        }
        let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    let ping = serde_json::to_string(&insync_core::ClientFrame::Ping)
                        .unwrap_or_default();
                    if ws.send(tokio_tungstenite::tungstenite::Message::Text(ping.into())).await.is_err() {
                        let _ = tx.send(WsDelivery::Failed);
                        return;
                    }
                }
                message = ws.next() => {
                    match message {
                        Some(Ok(message)) => {
                            if let Some(frame) = decode_frame(message)
                                && tx.send(WsDelivery::Frame(Box::new(frame))).is_err()
                            {
                                return;
                            }
                        }
                        _ => {
                            let _ = tx.send(WsDelivery::Failed);
                            return;
                        }
                    }
                }
            }
        }
    });
}

fn classify(error: crate::error::ClientError) -> OpOutcome {
    if error.kind() == crate::error::FailureKind::Permanent {
        OpOutcome::Drop("permanent failure")
    } else {
        OpOutcome::Retry
    }
}

fn now_ms() -> UnixMs {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn ws_url(base: &str, token: &str) -> String {
    let base = base.trim_end_matches('/');
    let scheme = if base.starts_with("https://") {
        "wss"
    } else {
        "ws"
    };
    let host = base
        .strip_prefix("https://")
        .or_else(|| base.strip_prefix("http://"))
        .unwrap_or(base);
    format!("{scheme}://{host}/v1/ws?token={token}")
}

fn decode_frame(
    message: tokio_tungstenite::tungstenite::Message,
) -> Option<insync_core::ServerFrame> {
    use tokio_tungstenite::tungstenite::Message;
    match message {
        Message::Text(text) => serde_json::from_str(&text).ok(),
        Message::Ping(_) | Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => None,
        Message::Close(_) => None,
    }
}
