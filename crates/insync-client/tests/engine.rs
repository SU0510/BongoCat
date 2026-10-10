//! Engine-level integration: two real sync engines against a real server,
//! exercising the full client stack (outbox, realtime push, state mirror).

use insync_client::{EngineConfig, EngineHandle, SyncCommand, SyncStatus};
use insync_core::SyncOp;
use insync_server::ServerConfig;
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};
use std::time::Duration;

struct TestServer {
    base: String,
    _dir: std::path::PathBuf,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._dir);
    }
}

async fn spawn_server() -> TestServer {
    let dir = std::env::temp_dir().join(format!(
        "insync-engine-{}-{}",
        std::process::id(),
        insync_core::new_client_id()
    ));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = ServerConfig {
        db_path: dir.join("insync.db"),
        bind_addr: format!("127.0.0.1:{port}").parse().unwrap(),
        token_ttl: Duration::from_secs(3600),
        wal: false,
    };
    tokio::spawn(async move {
        insync_server::serve(config).await.expect("server failed");
    });
    for _ in 0..100 {
        let client = reqwest::Client::new();
        if let Ok(resp) = client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
            && resp.status().is_success()
        {
            return TestServer {
                base: format!("http://127.0.0.1:{port}"),
                _dir: dir,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server not healthy");
}

async fn register(server: &TestServer, name: &str) -> (String, String) {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{}/v1/auth/register", server.base))
        .json(&json!({
            "username": name,
            "password": "engine-test-8",
            "display_name": name.to_uppercase(),
        }))
        .send()
        .await
        .unwrap();
    let body: Value = resp.json().await.unwrap();
    (
        body["token"].as_str().unwrap().to_string(),
        body["user"]["id"].as_str().unwrap().to_string(),
    )
}

async fn pair_users(server: &TestServer, token_a: &str, token_b: &str) {
    let rest = insync_client::InSyncClient::new(&server.base).with_token(token_a);
    let invite = rest.create_invite().await.unwrap();
    let rest_b = insync_client::InSyncClient::new(&server.base).with_token(token_b);
    rest_b.accept_invite(&invite.code).await.unwrap();
}

fn engine_for(server: &TestServer, token: Arc<RwLock<Option<String>>>) -> EngineHandle {
    EngineHandle::spawn(EngineConfig {
        server_url: server.base.clone(),
        token_source: Arc::new(move || {
            token
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }),
    })
}

fn wait_status(engine: &EngineHandle, want: SyncStatus, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if engine.state.status() == want {
            return true;
        }
        let _ = engine.recv_event_timeout(Duration::from_millis(100));
    }
    engine.state.status() == want
}

fn wait_event<F: Fn(&insync_client::EngineEvent) -> bool>(
    engine: &EngineHandle,
    matches: F,
    timeout: Duration,
) -> Option<insync_client::EngineEvent> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if std::time::Instant::now() > deadline {
            return None;
        }
        match engine.recv_event_timeout(Duration::from_millis(150)) {
            Some(event) if matches(&event) => return Some(event),
            Some(_) => continue,
            None => continue,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn two_engines_exchange_messages_pokes_and_focus() {
    let server = spawn_server().await;
    let (token_a, _id_a) = register(&server, "ega").await;
    let (token_b, _id_b) = register(&server, "egb").await;
    pair_users(&server, &token_a, &token_b).await;

    let token_a_arc = Arc::new(RwLock::new(Some(token_a)));
    let token_b_arc = Arc::new(RwLock::new(Some(token_b)));
    let engine_a = engine_for(&server, Arc::clone(&token_a_arc));
    let engine_b = engine_for(&server, Arc::clone(&token_b_arc));

    engine_a.command(SyncCommand::Connect);
    engine_b.command(SyncCommand::Connect);
    assert!(wait_status(
        &engine_a,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));
    assert!(wait_status(
        &engine_b,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));

    // A sends a message: B receives the realtime event
    engine_a.command(SyncCommand::Enqueue(SyncOp::SendMessage {
        client_id: insync_core::new_client_id(),
        body: "engine 晚安".into(),
    }));
    let got = wait_event(
        &engine_b,
        |event| {
            matches!(
                event,
                insync_client::EngineEvent::Event(frame)
                    if frame.kind == insync_core::EventKind::MessageCreated
            )
        },
        Duration::from_secs(20),
    );
    assert!(got.is_some(), "B must receive the realtime message event");

    // A's own mirror includes the message (echo via refresh/merge)
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let has = engine_a
            .state
            .read(|s| s.messages.iter().any(|m| m.body == "engine 晚安"));
        if has == Some(true) || std::time::Instant::now() > deadline {
            assert_eq!(has, Some(true), "sender's mirror includes the sent message");
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // poke: B receives the event
    engine_a.command(SyncCommand::Enqueue(SyncOp::SendPoke {
        client_id: insync_core::new_client_id(),
    }));
    let got = wait_event(
        &engine_b,
        |event| {
            matches!(
                event,
                insync_client::EngineEvent::Event(frame)
                    if frame.kind == insync_core::EventKind::PokeReceived
            )
        },
        Duration::from_secs(20),
    );
    assert!(got.is_some(), "B must receive the poke event");

    // focus lifecycle: invite -> accept -> finish early
    engine_a.command(SyncCommand::Enqueue(SyncOp::FocusInvite {
        client_id: insync_core::new_client_id(),
        topic: Some("engine 专注".into()),
        duration_secs: 300,
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut focus_id = None;
    while std::time::Instant::now() < deadline {
        focus_id = engine_b
            .state
            .read(|s| {
                s.active_focus
                    .as_ref()
                    .filter(|f| f.state == insync_core::FocusState::Invited)
                    .map(|f| f.id.clone())
            })
            .flatten();
        if focus_id.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let focus_id = focus_id.expect("B sees the invite");
    engine_b.command(SyncCommand::Enqueue(SyncOp::FocusAction {
        focus_id: focus_id.clone(),
        action: insync_core::FocusAction::Accept,
        base_revision: 1,
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut both_active = false;
    while std::time::Instant::now() < deadline {
        let a_active = engine_a.state.read(|s| {
            s.active_focus
                .as_ref()
                .is_some_and(|f| f.state == insync_core::FocusState::Active)
        });
        let b_active = engine_b.state.read(|s| {
            s.active_focus
                .as_ref()
                .is_some_and(|f| f.state == insync_core::FocusState::Active)
        });
        if a_active == Some(true) && b_active == Some(true) {
            both_active = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(both_active, "both engines converge on the active session");
    engine_b.command(SyncCommand::Enqueue(SyncOp::FocusAction {
        focus_id,
        action: insync_core::FocusAction::FinishEarly,
        base_revision: 2,
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        let cleared = engine_a.state.read(|s| s.active_focus.is_none());
        if cleared == Some(true) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        engine_a.state.read(|s| s.active_focus.is_none()),
        Some(true),
        "finished focus cleared on A"
    );

    engine_a.shutdown();
    engine_b.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn outbox_holds_ops_while_offline_and_drains_on_connect() {
    let server = spawn_server().await;
    let (token_a, _id_a) = register(&server, "egoa").await;
    let (token_b, _id_b) = register(&server, "egob").await;
    pair_users(&server, &token_a, &token_b).await;

    // A stays offline and queues a message + a letter
    let token_a_arc = Arc::new(RwLock::new(Some(token_a)));
    let token_b_arc = Arc::new(RwLock::new(Some(token_b)));
    let engine_a = engine_for(&server, Arc::clone(&token_a_arc));
    engine_a.command(SyncCommand::Enqueue(SyncOp::SendMessage {
        client_id: "offline-msg-1".into(),
        body: "我在离线时写的".into(),
    }));
    engine_a.command(SyncCommand::Enqueue(SyncOp::SendLetter {
        client_id: "offline-letter-1".into(),
        body: "离线留言".into(),
    }));
    std::thread::sleep(Duration::from_millis(300));

    let engine_b = engine_for(&server, Arc::clone(&token_b_arc));
    engine_b.command(SyncCommand::Connect);
    assert!(wait_status(
        &engine_b,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));

    // A connects: the outbox drains and B receives both
    engine_a.command(SyncCommand::Connect);
    assert!(wait_status(
        &engine_a,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));
    for want_kind in [
        insync_core::EventKind::MessageCreated,
        insync_core::EventKind::LetterCreated,
    ] {
        let got = wait_event(
            &engine_b,
            move |event| {
                matches!(
                    event,
                    insync_client::EngineEvent::Event(frame) if frame.kind == want_kind
                )
            },
            Duration::from_secs(20),
        );
        assert!(got.is_some(), "B received the offline op's event");
    }

    engine_a.shutdown();
    engine_b.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn shared_items_sync_and_note_conflict_surfaces() {
    let server = spawn_server().await;
    let (token_a, _id_a) = register(&server, "egsa").await;
    let (token_b, _id_b) = register(&server, "egsb").await;
    pair_users(&server, &token_a, &token_b).await;

    let token_a_arc = Arc::new(RwLock::new(Some(token_a)));
    let token_b_arc = Arc::new(RwLock::new(Some(token_b)));
    let engine_a = engine_for(&server, Arc::clone(&token_a_arc));
    let engine_b = engine_for(&server, Arc::clone(&token_b_arc));
    engine_a.command(SyncCommand::Connect);
    engine_b.command(SyncCommand::Connect);
    assert!(wait_status(
        &engine_a,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));
    assert!(wait_status(
        &engine_b,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));

    // shared todo created by A appears on B
    engine_a.command(SyncCommand::Enqueue(SyncOp::CreateTodo {
        client_id: insync_core::new_client_id(),
        title: "一起运动".into(),
        detail: None,
        priority: insync_core::Priority::High,
        due_at: None,
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut todo_id = None;
    while std::time::Instant::now() < deadline {
        todo_id = engine_b
            .state
            .read(|s| s.shared_todos.first().map(|t| t.id.clone()))
            .flatten();
        if todo_id.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let todo_id = todo_id.expect("B sees the shared todo");

    // B completes it; A's mirror shows the completion
    engine_b.command(SyncCommand::Enqueue(SyncOp::UpdateTodo {
        todo_id: todo_id.clone(),
        req: insync_core::api::UpdateTodoRequest {
            title: None,
            detail: None,
            priority: None,
            due_at: None,
            completed: Some(true),
        },
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        let done = engine_a.state.read(|s| {
            s.shared_todos
                .iter()
                .any(|t| t.id == todo_id && t.completed_at.is_some())
        });
        if done == Some(true) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        engine_a.state.read(|s| {
            s.shared_todos
                .iter()
                .any(|t| t.id == todo_id && t.completed_at.is_some())
        }),
        Some(true),
        "completion propagated"
    );

    // shared note with a revision conflict on B
    engine_a.command(SyncCommand::Enqueue(SyncOp::CreateNote {
        client_id: insync_core::new_client_id(),
        content: "第一版".into(),
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut note = None;
    while std::time::Instant::now() < deadline {
        note = engine_b
            .state
            .read(|s| s.shared_notes.first().cloned())
            .flatten();
        if note.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let note = note.expect("B sees the shared note");

    // A edits to revision 2; wait until B's mirror sees it so the next
    // patch is deterministically stale
    engine_a.command(SyncCommand::Enqueue(SyncOp::UpdateNote {
        note_id: note.id.clone(),
        content: "第二版".into(),
        base_revision: note.revision,
    }));
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        let revision = engine_b.state.read(|s| {
            s.shared_notes
                .iter()
                .find(|n| n.id == note.id)
                .map(|n| n.revision)
        });
        if revision == Some(Some(note.revision + 1)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // B patches with a stale revision -> conflict event, nothing lost
    engine_b.command(SyncCommand::Enqueue(SyncOp::UpdateNote {
        note_id: note.id.clone(),
        content: "B 的旧编辑".into(),
        base_revision: note.revision,
    }));
    let got = wait_event(
        &engine_b,
        |event| matches!(event, insync_client::EngineEvent::NoteConflict { .. }),
        Duration::from_secs(20),
    );
    assert!(
        got.is_some(),
        "the conflict must surface, not silently drop"
    );
    assert_eq!(
        engine_b
            .state
            .read(|s| s
                .shared_notes
                .iter()
                .find(|n| n.id == note.id)
                .map(|n| n.content.clone()))
            .flatten(),
        Some("第二版".to_string())
    );

    engine_a.shutdown();
    engine_b.shutdown();
}

#[tokio::test(flavor = "multi_thread")]
async fn dissolution_clears_shared_state() {
    let server = spawn_server().await;
    let (token_a, _id_a) = register(&server, "egda").await;
    let (token_b, _id_b) = register(&server, "egdb").await;
    pair_users(&server, &token_a, &token_b).await;

    let token_a_arc = Arc::new(RwLock::new(Some(token_a)));
    let token_b_arc = Arc::new(RwLock::new(Some(token_b)));
    let engine_a = engine_for(&server, Arc::clone(&token_a_arc));
    let engine_b = engine_for(&server, Arc::clone(&token_b_arc));
    engine_a.command(SyncCommand::Connect);
    engine_b.command(SyncCommand::Connect);
    assert!(wait_status(
        &engine_a,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));
    assert!(wait_status(
        &engine_b,
        SyncStatus::Online,
        Duration::from_secs(20)
    ));

    // B dissolves via REST; A's engine clears the shared mirror
    let client = reqwest::Client::new();
    let token_b_snapshot = token_b_arc
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .unwrap();
    let resp = client
        .post(format!("{}/v1/pair/dissolve", server.base))
        .bearer_auth(token_b_snapshot)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    let got = wait_event(
        &engine_a,
        |event| {
            matches!(
                event,
                insync_client::EngineEvent::Event(frame)
                    if frame.kind == insync_core::EventKind::RelationDissolved
            )
        },
        Duration::from_secs(20),
    );
    assert!(got.is_some(), "A learns about the dissolution");
    assert_eq!(
        engine_a
            .state
            .read(|s| { s.relation.is_none() && s.partner.is_none() && s.shared_todos.is_empty() }),
        Some(true),
        "shared state cleared locally too"
    );

    engine_a.shutdown();
    engine_b.shutdown();
}
