pub mod auth;
pub mod chat;
pub mod focus;
pub mod items;
pub mod pair;
pub mod presence;

use crate::error::ApiError;
use insync_core::{CoupleRelation, RelationStatus};
use rusqlite::Connection;

/// The user's most recent relation, regardless of status. Callers that care
/// about pairing eligibility must filter on [`CoupleRelation::is_active`].
pub fn relation_for(conn: &Connection, user_id: &str) -> Result<Option<CoupleRelation>, ApiError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, user_a, user_b, status, created_at, last_interaction_at, dissolved_at
             FROM relations
             WHERE (user_a = ?1 OR user_b = ?1)
             ORDER BY created_at DESC LIMIT 1",
        )
        .map_err(ApiError::from)?;
    let mut rows = stmt
        .query_map([user_id], map_relation)
        .map_err(ApiError::from)?;
    rows.next().transpose().map_err(ApiError::from)
}

fn map_relation(row: &rusqlite::Row<'_>) -> Result<CoupleRelation, rusqlite::Error> {
    let status: String = row.get(3)?;
    Ok(CoupleRelation {
        id: row.get(0)?,
        user_a: row.get(1)?,
        user_b: row.get(2)?,
        status: if status == "active" {
            RelationStatus::Active
        } else {
            RelationStatus::Dissolved
        },
        created_at: row.get(4)?,
        last_interaction_at: row.get(5)?,
        dissolved_at: row.get(6)?,
    })
}

/// Requires an active relation; returns the relation and the partner id.
/// Shared access is revoked here, not in the client.
pub fn require_active_relation(
    conn: &Connection,
    user_id: &str,
) -> Result<(CoupleRelation, String), ApiError> {
    let relation = match relation_for(conn, user_id)? {
        Some(r) => r,
        None => return Err(ApiError::forbidden("no partner relationship")),
    };
    if !relation.is_active() {
        return Err(ApiError::relation_inactive());
    }
    let partner = relation
        .partner_of(user_id)
        .ok_or_else(|| ApiError::forbidden("not a member of this relation"))?
        .to_string();
    Ok((relation, partner))
}

/// Touches `last_interaction_at` so the UI can show relationship activity.
pub fn touch_relation(conn: &Connection, relation_id: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "UPDATE relations SET last_interaction_at = ?1 WHERE id = ?2",
        rusqlite::params![crate::db::Db::now_ms(), relation_id],
    )?;
    Ok(())
}

pub fn public_user(
    conn: &Connection,
    user_id: &str,
) -> Result<insync_core::PublicUser, rusqlite::Error> {
    conn.query_row(
        "SELECT id, username, display_name, created_at FROM users WHERE id = ?1",
        [user_id],
        |row| {
            Ok(insync_core::PublicUser {
                id: row.get(0)?,
                username: row.get(1)?,
                display_name: row.get(2)?,
                created_at: row.get(3)?,
            })
        },
    )
}
