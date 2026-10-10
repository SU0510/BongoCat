//! The realtime WebSocket: bearer auth, hello/ready handshake, event push,
//! heartbeats and presence lifecycle.

use crate::auth::AuthUser;
use crate::events;
use crate::routes::presence;
use crate::state::AppState;
use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use insync_core::{ClientFrame, ServerFrame};
use std::collections::HashMap;
use std::time::Duration;

/// Bearer token via header (handled before upgrade) or `?token=` for
/// clients that cannot set headers during the upgrade request.
pub async fn upgrade(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string)
        .or_else(|| q.get("token").cloned());
    let Some(token) = token else {
        return crate::error::ApiError::unauthorized("missing bearer token").into_response();
    };
    // a token check is a single indexed SQLite read; blocking briefly here
    // is cheaper than shuffling to a worker pool
    let Ok(user) = crate::auth::authenticate_token(&state, &token) else {
        return crate::error::ApiError::unauthorized("invalid or expired token").into_response();
    };
    // non-members must not complete the upgrade at all
    {
        let conn = state.db.lock();
        let member = crate::routes::relation_for(&conn, &user.id)
            .ok()
            .flatten()
            .is_some_and(|r| r.is_active());
        if !member {
            return crate::error::ApiError::relation_inactive().into_response();
        }
    }
    ws.on_upgrade(move |socket| async move {
        handle_socket(state, user, socket).await;
    })
}

async fn handle_socket(state: AppState, user: AuthUser, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();

    // the relation gates everything; without one the socket is closed
    let relation = {
        let conn = state.db.lock();
        crate::routes::relation_for(&conn, &user.id).ok().flatten()
    };
    let Some(relation) = relation.filter(|r| r.is_active()) else {
        let _ = sink
            .send(server_json(&ServerFrame::Error {
                code: insync_core::api::error_codes::RELATION_INACTIVE.into(),
                message: "no active relationship".into(),
            }))
            .await;
        return;
    };
    let relation_id = relation.id.clone();

    // mark online and notify the partner
    {
        let conn = state.db.lock();
        let _ = presence::upsert_presence(
            &conn,
            &user.id,
            insync_core::PresenceState::Online,
            true,
            crate::db::Db::now_ms(),
        );
        if let Ok(Some(p)) = crate::routes::chat::load_presence(&conn, &user.id) {
            let _ = events::emit(
                &state,
                &conn,
                &relation_id,
                insync_core::EventKind::PresenceUpdated,
                &user.id,
                insync_core::EventData::Presence(Box::new(p)),
            );
        }
    }

    let mut rx = state.bus.subscribe(&relation_id);

    // --- hello handshake ---------------------------------------------------
    // Wait briefly for hello so catch-up starts from the client's cursor.
    let mut last_seq: Option<u64> = None;
    if let Ok(Some(Ok(WsMessage::Text(text)))) =
        tokio::time::timeout(Duration::from_secs(5), stream.next()).await
        && let Ok(ClientFrame::Hello { last_seq: seq }) = serde_json::from_str(&text)
    {
        last_seq = seq;
    }
    let latest = {
        let conn = state.db.lock();
        events::latest_seq(&conn, &relation_id).unwrap_or(0)
    };
    let _ = sink
        .send(server_json(&ServerFrame::Ready { latest_seq: latest }))
        .await;
    if let Some(cursor) = last_seq {
        // replay everything the client missed while offline
        let missed = {
            let conn = state.db.lock();
            events::load_after(&conn, &relation_id, cursor, 500).unwrap_or_default()
        };
        for frame in missed {
            if sink
                .send(server_json(&ServerFrame::Event(Box::new(frame))))
                .await
                .is_err()
            {
                return;
            }
        }
    }

    // --- main loop: pump bus events + client frames -------------------------
    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                presence::touch_online(&state, &user.id);
                if sink.send(WsMessage::Ping(axum::body::Bytes::new())).await.is_err() {
                    break;
                }
            }
            frame = rx.recv() => {
                match frame {
                    Ok(event) => {
                        if sink.send(server_json(&ServerFrame::Event(Box::new(event)))).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // subscriber fell behind: tell it to resync via REST
                        let latest = {
                            let conn = state.db.lock();
                            events::latest_seq(&conn, &relation_id).unwrap_or(0)
                        };
                        let _ = sink.send(server_json(&ServerFrame::Error {
                            code: "EVENTS_LAGGED".into(),
                            message: format!("missed {n} events; resync from seq {latest}"),
                        })).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            msg = stream.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(frame) = serde_json::from_str::<ClientFrame>(&text) {
                            match frame {
                                ClientFrame::Ping => {
                                    if sink.send(server_json(&ServerFrame::Pong)).await.is_err() {
                                        break;
                                    }
                                    presence::touch_online(&state, &user.id);
                                }
                                ClientFrame::Hello { .. } => {
                                    // repeated hellos are ignored; catch-up happens once
                                }
                            }
                        }
                    }
                    Some(Ok(WsMessage::Pong(_))) => {
                        presence::touch_online(&state, &user.id);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    // --- offline transition ---------------------------------------------------
    presence::mark_offline(&state, &user.id);
}

fn server_json(frame: &ServerFrame) -> WsMessage {
    let json = serde_json::to_string(frame).unwrap_or_else(|_| "{}".to_string());
    WsMessage::Text(json.into())
}
