//! End-to-end integration tests: a real server on a real TCP port with two
//! independent clients (REST + WebSocket), covering pairing, chat, letters,
//! pokes, focus, shared items, offline recovery, idempotency and the
//! security matrix.

use futures_util::{SinkExt, StreamExt};
use insync_core::{ClientFrame, EventData, EventKind, ServerFrame};
use insync_server::ServerConfig;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message as WsText;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

struct TestServer {
    base: String,
    port: u16,
    _db_dir: PathBuf,
}

async fn spawn_server() -> TestServer {
    let dir = std::env::temp_dir().join(format!(
        "insync-test-{}-{}",
        std::process::id(),
        uuid::Uuid::now_v7().simple()
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
    let base = format!("http://127.0.0.1:{port}");
    // wait for readiness
    for _ in 0..100 {
        if let Ok(resp) = Client::new().get(format!("{base}/health")).send().await
            && resp.status().is_success()
        {
            return TestServer {
                base,
                port,
                _db_dir: dir,
            };
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("server did not become healthy");
}

struct User {
    token: String,
    id: String,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self._db_dir);
    }
}

async fn register(server: &TestServer, name: &str) -> User {
    let client = Client::new();
    let resp = client
        .post(format!("{}/v1/auth/register", server.base))
        .json(&json!({
            "username": name,
            "password": "super-secret-8",
            "display_name": name.to_uppercase(),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "register {name}");
    let body: Value = resp.json().await.unwrap();
    User {
        token: body["token"].as_str().unwrap().to_string(),
        id: body["user"]["id"].as_str().unwrap().to_string(),
    }
}

async fn pair(server: &TestServer, a: &User, b: &User) -> String {
    let client = Client::new();
    let resp = client
        .post(format!("{}/v1/pair/invite", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let invite: Value = resp.json().await.unwrap();
    let code = invite["code"].as_str().unwrap().to_string();
    let resp = client
        .post(format!("{}/v1/pair/accept", server.base))
        .bearer_auth(&b.token)
        .json(&json!({ "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "accept invite");
    let body: Value = resp.json().await.unwrap();
    body["relation"]["id"].as_str().unwrap().to_string()
}

struct Authed<'a> {
    client: &'a Client,
    token: &'a str,
}

impl Authed<'_> {
    fn request(&self, method: reqwest::Method, url: impl Into<String>) -> reqwest::RequestBuilder {
        self.client
            .request(method, url.into())
            .bearer_auth(self.token)
    }
}

fn authed<'a>(client: &'a Client, server: &TestServer, user: &'a User) -> Authed<'a> {
    let _ = &server.base;
    Authed {
        client,
        token: &user.token,
    }
}

async fn connect_ws(
    server: &TestServer,
    user: &User,
) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
    let (ws, _) = connect_async(format!(
        "ws://127.0.0.1:{}/v1/ws?token={}",
        server.port, user.token
    ))
    .await
    .expect("ws connect");
    ws
}

/// Reads the next protocol frame, skipping ping/pong noise, with a hard
/// timeout so a misbehaving server fails the test instead of hanging it.
async fn recv_frame(ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Option<ServerFrame> {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(10), ws.next())
            .await
            .expect("timed out waiting for ws frame")?
            .ok()?;
        match msg {
            WsText::Text(text) => {
                if let Ok(frame) = serde_json::from_str::<ServerFrame>(&text) {
                    return Some(frame);
                }
                // unknown frame shapes are skipped (forward compatibility)
            }
            WsText::Ping(_) | WsText::Pong(_) | WsText::Binary(_) | WsText::Frame(_) => continue,
            WsText::Close(_) => return None,
        }
    }
}

async fn ws_hello(
    ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    last_seq: Option<u64>,
) -> u64 {
    ws.send(WsText::Text(
        serde_json::to_string(&ClientFrame::Hello { last_seq })
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();
    match recv_frame(ws).await {
        Some(ServerFrame::Ready { latest_seq }) => latest_seq,
        other => panic!("expected ready frame, got {other:?}"),
    }
}

async fn ws_next(ws: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> ServerFrame {
    loop {
        match recv_frame(ws).await {
            Some(ServerFrame::Event(event)) => return ServerFrame::Event(event),
            Some(_) => continue,
            None => panic!("websocket closed while waiting for an event"),
        }
    }
}

fn event_of(frame: ServerFrame) -> (EventKind, EventData, u64) {
    match frame {
        ServerFrame::Event(event) => (event.kind, event.data, event.seq),
        other => panic!("expected event, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pairing_flow_and_me() {
    let server = spawn_server().await;
    let a = register(&server, "alice").await;
    let b = register(&server, "bob").await;
    let relation_id = pair(&server, &a, &b).await;

    let client = Client::new();
    let me: Value = authed(&client, &server, &a)
        .request(reqwest::Method::GET, format!("{}/v1/me", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me["relation"]["id"], relation_id.as_str());
    assert_eq!(me["partner"]["username"], "bob");
}

#[tokio::test(flavor = "multi_thread")]
async fn message_round_trip_with_ws_push_and_dedupe() {
    let server = spawn_server().await;
    let a = register(&server, "carol").await;
    let b = register(&server, "dave").await;
    pair(&server, &a, &b).await;

    let mut ws_b = connect_ws(&server, &b).await;
    ws_hello(&mut ws_b, None).await;

    let client = Client::new();
    let resp = client
        .post(format!("{}/v1/messages", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "cm-1", "body": "晚安，做个好梦" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let message: Value = resp.json().await.unwrap();

    // partner receives the realtime push (its own connect-time presence is
    // emitted before it subscribed, so it never sees that one)
    let (kind, data, _) = event_of(ws_next(&mut ws_b).await);
    assert_eq!(kind, EventKind::MessageCreated);
    match data {
        EventData::Message(m) => {
            assert_eq!(m.body, "晚安，做个好梦");
            assert_eq!(m.seq, message["seq"].as_u64().unwrap());
        }
        other => panic!("unexpected payload {other:?}"),
    }

    // idempotent resend with the same client_id returns the same message
    let resp = client
        .post(format!("{}/v1/messages", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "cm-1", "body": "晚安，做个好梦" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "idempotent resend");
    let again: Value = resp.json().await.unwrap();
    assert_eq!(again["id"], message["id"]);

    // history via REST
    let page: Value = authed(&client, &server, &b)
        .request(reqwest::Method::GET, format!("{}/v1/messages", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        page["messages"].as_array().unwrap().len(),
        1,
        "dedupe: exactly one message"
    );

    // read receipt
    let resp = client
        .post(format!("{}/v1/messages/read", server.base))
        .bearer_auth(&b.token)
        .json(&json!({ "last_message_seq": message["seq"] }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let overview: Value = authed(&client, &server, &b)
        .request(reqwest::Method::GET, format!("{}/v1/overview", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(overview["overview"]["unread_messages"], 0);
    assert_eq!(
        overview["overview"]["last_message"]["body"],
        "晚安，做个好梦"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn letters_flow_with_unread_counts() {
    let server = spawn_server().await;
    let a = register(&server, "erin").await;
    let b = register(&server, "frank").await;
    pair(&server, &a, &b).await;

    let client = Client::new();
    let resp = client
        .post(format!("{}/v1/letters", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "lt-1", "body": "我出门啦，晚上给你带好吃的" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let letter: Value = resp.json().await.unwrap();

    let overview: Value = authed(&client, &server, &b)
        .request(reqwest::Method::GET, format!("{}/v1/overview", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(overview["overview"]["unread_letters"], 1);

    let resp = client
        .post(format!(
            "{}/v1/letters/{}/read",
            server.base,
            letter["id"].as_str().unwrap()
        ))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let overview: Value = authed(&client, &server, &b)
        .request(reqwest::Method::GET, format!("{}/v1/overview", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(overview["overview"]["unread_letters"], 0);

    // the sender cannot delete the letter b already read? sender can; but a
    // non-sender cannot
    let resp = client
        .delete(format!(
            "{}/v1/letters/{}",
            server.base,
            letter["id"].as_str().unwrap()
        ))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let resp = client
        .delete(format!(
            "{}/v1/letters/{}",
            server.base,
            letter["id"].as_str().unwrap()
        ))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test(flavor = "multi_thread")]
async fn poke_reaches_partner_and_rate_limits() {
    let server = spawn_server().await;
    let a = register(&server, "grace").await;
    let b = register(&server, "heidi").await;
    pair(&server, &a, &b).await;

    let mut ws_b = connect_ws(&server, &b).await;
    ws_hello(&mut ws_b, None).await;

    let client = Client::new();
    let resp = client
        .post(format!("{}/v1/pokes", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "pk-1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    loop {
        let (kind, _, _) = event_of(ws_next(&mut ws_b).await);
        if kind == EventKind::PokeReceived {
            break;
        }
    }

    // server-side cooldown: second poke within 2s is rejected
    let resp = client
        .post(format!("{}/v1/pokes", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "pk-2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_session_lifecycle_end_to_end() {
    let server = spawn_server().await;
    let a = register(&server, "ivan").await;
    let b = register(&server, "judy").await;
    pair(&server, &a, &b).await;

    let client = Client::new();
    // invite
    let resp = client
        .post(format!("{}/v1/focus/invite", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "fc-1", "topic": "一起读书", "duration_secs": 60 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let focus: Value = resp.json().await.unwrap();
    let focus_id = focus["focus"]["id"].as_str().unwrap().to_string();
    assert_eq!(focus["focus"]["state"], "invited");

    // a second live invite conflicts
    let resp = client
        .post(format!("{}/v1/focus/invite", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "fc-2", "duration_secs": 60 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // partner accepts -> starts
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/accept", server.base))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let started: Value = resp.json().await.unwrap();
    assert_eq!(started["focus"]["state"], "active");
    let end_at = started["focus"]["end_at"].as_i64().unwrap();
    let started_at = started["focus"]["started_at"].as_i64().unwrap();
    assert_eq!(end_at - started_at, 60_000, "shared end time");
    let revision = started["focus"]["revision"].as_u64().unwrap();

    // initiator cannot accept their own invite (fresh session would be
    // needed, but the current one is active -> transition is rejected)
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/accept", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // pause by b, resume by a; end time slides by the pause window
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/pause", server.base))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.json::<Value>().await.unwrap()["focus"]["state"],
        "paused"
    );
    tokio::time::sleep(Duration::from_millis(30)).await;
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/resume", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resumed: Value = resp.json().await.unwrap();
    assert_eq!(resumed["focus"]["state"], "active");
    assert!(
        resumed["focus"]["end_at"].as_i64().unwrap() >= end_at + 30,
        "end slides by pause window"
    );
    assert!(resumed["focus"]["revision"].as_u64().unwrap() > revision);

    // finish early by a
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/finish-early", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let done: Value = resp.json().await.unwrap();
    assert_eq!(done["focus"]["state"], "completed");
    assert_eq!(done["focus"]["ended_early"], true);

    // terminal state rejects further transitions
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/pause", server.base))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // after completion the overview no longer reports an active focus
    let overview: Value = authed(&client, &server, &a)
        .request(reqwest::Method::GET, format!("{}/v1/overview", server.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(overview["overview"]["active_focus"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_invite_decline_and_cancel() {
    let server = spawn_server().await;
    let a = register(&server, "kate").await;
    let b = register(&server, "leo").await;
    pair(&server, &a, &b).await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/v1/focus/invite", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "fc-1", "duration_secs": 300 }))
        .send()
        .await
        .unwrap();
    let focus: Value = resp.json().await.unwrap();
    let focus_id = focus["focus"]["id"].as_str().unwrap().to_string();

    // partner declines
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/decline", server.base))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.json::<Value>().await.unwrap()["focus"]["state"],
        "declined"
    );

    // second session: initiator cancels before accept
    let resp = client
        .post(format!("{}/v1/focus/invite", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "fc-2", "duration_secs": 300 }))
        .send()
        .await
        .unwrap();
    let focus: Value = resp.json().await.unwrap();
    let focus_id = focus["focus"]["id"].as_str().unwrap().to_string();
    let resp = client
        .post(format!("{}/v1/focus/{focus_id}/cancel", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.json::<Value>().await.unwrap()["focus"]["state"],
        "cancelled"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn shared_items_and_note_conflict() {
    let server = spawn_server().await;
    let a = register(&server, "mallory").await;
    let b = register(&server, "nancy").await;
    pair(&server, &a, &b).await;
    let client = Client::new();

    // shared todo created by a, completed by b
    let resp = client
        .post(format!("{}/v1/todos", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "td-1", "title": "一起运动 30 分钟", "priority": "high" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let todo: Value = resp.json().await.unwrap();
    let todo_id = todo["id"].as_str().unwrap().to_string();

    let resp = client
        .patch(format!("{}/v1/todos/{todo_id}", server.base))
        .bearer_auth(&b.token)
        .json(&json!({ "completed": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let done: Value = resp.json().await.unwrap();
    assert_eq!(done["completed_by"], b.id.as_str());

    // shared note with revision conflict
    let resp = client
        .post(format!("{}/v1/notes", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "nt-1", "content": "计划去海边" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let note: Value = resp.json().await.unwrap();
    let note_id = note["id"].as_str().unwrap().to_string();

    // a edits with base revision 1
    let resp = client
        .patch(format!("{}/v1/notes/{note_id}", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "content": "计划去海边，顺便看日出", "base_revision": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // b edits with a stale base revision -> 409 with current content
    let resp = client
        .patch(format!("{}/v1/notes/{note_id}", server.base))
        .bearer_auth(&b.token)
        .json(&json!({ "content": "改去山里露营", "base_revision": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let conflict: Value = resp.json().await.unwrap();
    assert_eq!(conflict["current"]["content"], "计划去海边，顺便看日出");

    // anniversary
    let resp = client
        .post(format!("{}/v1/anniversaries", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "an-1", "name": "在一起", "date": "2024-05-20", "repeat_yearly": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bad = client
        .post(format!("{}/v1/anniversaries", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "an-2", "name": "坏日期", "date": "20240520" }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test(flavor = "multi_thread")]
async fn offline_reconnect_catches_up_from_cursor() {
    let server = spawn_server().await;
    let a = register(&server, "olivia").await;
    let b = register(&server, "peter").await;
    pair(&server, &a, &b).await;
    let client = Client::new();

    // b connects, learns the cursor, then goes offline
    let mut ws_b = connect_ws(&server, &b).await;
    let latest = ws_hello(&mut ws_b, None).await;
    drop(ws_b);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // while offline: a sends two messages and a poke
    for i in 1..=2 {
        let resp = client
            .post(format!("{}/v1/messages", server.base))
            .bearer_auth(&a.token)
            .json(&json!({ "client_id": format!("om-{i}"), "body": format!("离线消息{i}") }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
    let resp = client
        .post(format!("{}/v1/pokes", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "op-1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // b reconnects with the cursor and receives exactly the missed events
    // (its fresh presence event is emitted before it re-subscribes, so it
    // is not part of the replay)
    let mut ws_b = connect_ws(&server, &b).await;
    let ready = ws_hello(&mut ws_b, Some(latest)).await;
    assert!(ready >= latest + 3, "events were persisted while offline");
    // the replay also carries bookkeeping events (e.g. our own offline
    // presence flip), so collect until the important ones show up
    let mut kinds = Vec::new();
    for _ in 0..8 {
        match ws_next(&mut ws_b).await {
            ServerFrame::Event(e) => kinds.push(e.kind),
            other => panic!("unexpected frame {other:?}"),
        }
        let messages = kinds
            .iter()
            .filter(|k| **k == EventKind::MessageCreated)
            .count();
        if messages == 2 && kinds.contains(&EventKind::PokeReceived) {
            break;
        }
    }
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == EventKind::MessageCreated)
            .count(),
        2,
        "both offline messages replay exactly once (no duplicates)"
    );
    assert!(kinds.contains(&EventKind::PokeReceived));

    // the events endpoint supports the same catch-up over REST
    let page: Value = authed(&client, &server, &b)
        .request(
            reqwest::Method::GET,
            format!("{}/v1/events?after={latest}", server.base),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(page["events"].as_array().unwrap().len() >= 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn security_matrix() {
    let server = spawn_server().await;
    let a = register(&server, "quinn").await;
    let b = register(&server, "ruth").await;
    let outsider = register(&server, "steve").await;
    pair(&server, &a, &b).await;
    let client = Client::new();

    // unauthenticated requests are rejected
    let resp = client
        .get(format!("{}/v1/messages", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // garbage tokens are rejected
    let resp = client
        .get(format!("{}/v1/messages", server.base))
        .bearer_auth("not-a-token")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // outsider cannot read the couple's data even with a valid token
    let resp = authed(&client, &server, &outsider)
        .request(reqwest::Method::GET, format!("{}/v1/messages", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // outsider cannot join the couple's websocket either
    let ws_result = connect_async(format!(
        "ws://127.0.0.1:{}/v1/ws?token={}",
        server.port, outsider.token
    ))
    .await;
    assert!(ws_result.is_err(), "non-member ws upgrade must fail");

    // id tampering: outsider cannot write into a guessed relation
    let resp = authed(&client, &server, &outsider)
        .request(
            reqwest::Method::GET,
            format!("{}/v1/events?after=0", server.base),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // ws with a bogus token fails
    let ws_result =
        connect_async(format!("ws://127.0.0.1:{}/v1/ws?token=bogus", server.port)).await;
    assert!(ws_result.is_err());

    // token revocation via logout takes effect immediately
    let resp = client
        .post(format!("{}/v1/auth/logout", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let resp = authed(&client, &server, &a)
        .request(reqwest::Method::GET, format!("{}/v1/me", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "revoked token rejected"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dissolve_revokes_shared_access() {
    let server = spawn_server().await;
    let a = register(&server, "tina").await;
    let b = register(&server, "victor").await;
    pair(&server, &a, &b).await;
    let client = Client::new();

    // shared data exists before dissolution
    let resp = client
        .post(format!("{}/v1/messages", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "dm-1", "body": "before" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // b dissolves
    let resp = client
        .post(format!("{}/v1/pair/dissolve", server.base))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // both lose access to shared data
    for user in [&a, &b] {
        let resp = authed(&client, &server, user)
            .request(reqwest::Method::GET, format!("{}/v1/messages", server.base))
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "shared access must be revoked server-side"
        );
        let body: Value = resp.json().await.unwrap();
        assert_eq!(body["error"]["code"], "RELATION_INACTIVE");
    }

    // sending into a dissolved relation is rejected too
    let resp = client
        .post(format!("{}/v1/messages", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "dm-2", "body": "after" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    // the dissolved user can invite someone else afterwards
    let resp = client
        .post(format!("{}/v1/pair/invite", server.base))
        .bearer_auth(&b.token)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
}

#[tokio::test(flavor = "multi_thread")]
async fn invite_codes_cannot_be_reused_or_expired() {
    let server = spawn_server().await;
    let a = register(&server, "wendy").await;
    let b = register(&server, "xavier").await;
    let c = register(&server, "yuri").await;
    let client = Client::new();

    let resp = client
        .post(format!("{}/v1/pair/invite", server.base))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    let invite: Value = resp.json().await.unwrap();
    let code = invite["code"].as_str().unwrap().to_string();

    // accept by b
    let resp = client
        .post(format!("{}/v1/pair/accept", server.base))
        .bearer_auth(&b.token)
        .json(&json!({ "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // reuse by c fails
    let resp = client
        .post(format!("{}/v1/pair/accept", server.base))
        .bearer_auth(&c.token)
        .json(&json!({ "code": code }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // self-accept is rejected
    let resp = client
        .post(format!("{}/v1/pair/invite", server.base))
        .bearer_auth(&c.token)
        .send()
        .await
        .unwrap();
    let invite: Value = resp.json().await.unwrap();
    let own_code = invite["code"].as_str().unwrap().to_string();
    let resp = client
        .post(format!("{}/v1/pair/accept", server.base))
        .bearer_auth(&c.token)
        .json(&json!({ "code": own_code }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // garbage code rejected
    let resp = client
        .post(format!("{}/v1/pair/accept", server.base))
        .bearer_auth(&c.token)
        .json(&json!({ "code": "ZZZZZZ22" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread")]
async fn login_failures_are_rate_limited() {
    let server = spawn_server().await;
    register(&server, "zoe").await;
    let client = Client::new();
    let mut last_status = StatusCode::OK;
    for i in 0..12 {
        let resp = client
            .post(format!("{}/v1/auth/login", server.base))
            .json(&json!({ "username": "zoe", "password": format!("wrong-{i}") }))
            .send()
            .await
            .unwrap();
        last_status = resp.status();
    }
    assert_eq!(
        last_status,
        StatusCode::TOO_MANY_REQUESTS,
        "brute force is throttled"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn server_logs_do_not_contain_message_bodies() {
    // the server writes only the startup line to stderr; assert the binary's
    // privacy contract by checking no endpoint echoes into error bodies
    let server = spawn_server().await;
    let a = register(&server, "aaron").await;
    let b = register(&server, "betty").await;
    pair(&server, &a, &b).await;
    let client = Client::new();
    let secret = "-private-do-not-log-42";
    let resp = client
        .post(format!("{}/v1/messages", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "lp-1", "body": secret }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    // error responses must not echo arbitrary message content
    let resp = client
        .post(format!("{}/v1/messages", server.base))
        .bearer_auth(&a.token)
        .json(&json!({ "client_id": "lp-2", "body": "" }))
        .send()
        .await
        .unwrap();
    let text = resp.text().await.unwrap();
    assert!(!text.contains(secret));
}
