//! Event persistence + fan-out. Events are written inside the caller's
//! transaction scope, assigned a per-relation seq, then broadcast.

use crate::db::Db;
use crate::state::AppState;
use insync_core::{EventData, EventFrame, EventKind};
use rusqlite::Connection;

/// Inserts an event for `relation_id` and broadcasts it. Returns the frame
/// with its assigned seq.
pub fn emit(
    state: &AppState,
    conn: &Connection,
    relation_id: &str,
    kind: EventKind,
    actor_id: &str,
    data: EventData,
) -> Result<EventFrame, rusqlite::Error> {
    let now = Db::now_ms();
    let seq: i64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM events WHERE relation_id = ?1",
        [relation_id],
        |row| row.get(0),
    )?;
    let id = uuid::Uuid::now_v7().to_string();
    let payload = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());
    conn.execute(
        "INSERT INTO events (seq, id, relation_id, kind, actor_id, server_time, version, payload)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            seq,
            id,
            relation_id,
            kind.as_str(),
            actor_id,
            now,
            insync_core::PROTOCOL_VERSION,
            payload
        ],
    )?;
    let frame = EventFrame::new(
        seq as u64,
        id,
        kind,
        actor_id.to_string(),
        relation_id.to_string(),
        now,
        data,
    );
    state.bus.publish(&frame);
    Ok(frame)
}

/// Loads events after `after_seq` for catch-up.
pub fn load_after(
    conn: &Connection,
    relation_id: &str,
    after_seq: u64,
    limit: i64,
) -> Result<Vec<EventFrame>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT seq, id, kind, actor_id, server_time, version, payload
         FROM events WHERE relation_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![relation_id, after_seq as i64, limit],
        |row| {
            let payload: String = row.get(6)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                payload,
            ))
        },
    )?;
    let mut frames = Vec::new();
    for row in rows {
        let (seq, id, kind, actor_id, server_time, version, payload) = row?;
        let Some(kind) = EventKind::parse(&kind) else {
            continue; // unknown kinds are skipped on catch-up as on the wire
        };
        let data: EventData = serde_json::from_str(&payload).unwrap_or_default();
        frames.push(EventFrame {
            seq: seq as u64,
            id,
            kind,
            actor_id,
            relation_id: relation_id.to_string(),
            server_time,
            version: version as u32,
            data,
        });
    }
    Ok(frames)
}

/// Latest seq known for a relation (0 = empty log).
pub fn latest_seq(conn: &Connection, relation_id: &str) -> Result<u64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM events WHERE relation_id = ?1",
        [relation_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v as u64)
}

/// Retention pruning: short-lived interaction events older than the window
/// are removed; the log for durable data keeps its tail bounded per
/// relation. `durable_keep` bounds how many durable events stay for
/// catch-up (clients resync state via REST beyond that).
pub fn prune(
    conn: &Connection,
    ephemeral_max_age_ms: i64,
    durable_keep: i64,
    now: i64,
) -> Result<(usize, usize), rusqlite::Error> {
    let ephemeral = conn.execute(
        "DELETE FROM events WHERE kind IN ('poke.received', 'presence.updated')
         AND server_time < ?1",
        [now - ephemeral_max_age_ms],
    )?;
    let durable = conn.execute(
        "DELETE FROM events WHERE kind NOT IN ('poke.received', 'presence.updated')
         AND seq <= (SELECT COALESCE(MAX(seq), 0) - ?1 FROM events e2
                     WHERE e2.relation_id = events.relation_id)",
        [durable_keep],
    )?;
    Ok((ephemeral, durable))
}
