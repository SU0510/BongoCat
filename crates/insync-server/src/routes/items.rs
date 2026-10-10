//! Shared items: todos, notes (with revision conflict), anniversaries.

use crate::auth::AuthUser;
use crate::db::Db;
use crate::error::{ApiError, is_unique_violation};
use crate::events;
use crate::routes::require_active_relation;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use insync_core::api::{
    AnniversaryPage, CreateAnniversaryRequest, CreateNoteRequest, CreateTodoRequest, NoteConflict,
    NotePage, TodoPage, UpdateAnniversaryRequest, UpdateNoteRequest, UpdateTodoRequest,
};
use insync_core::{EventData, EventKind, Priority, SharedAnniversary, SharedNote, SharedTodo};
use rusqlite::params;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/todos", get(list_todos).post(create_todo))
        .route("/todos/{id}", patch_todo_route().delete(delete_todo))
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/{id}", patch_note_route().delete(delete_note))
        .route(
            "/anniversaries",
            get(list_anniversaries).post(create_anniversary),
        )
        .route(
            "/anniversaries/{id}",
            patch_anniversary_route().delete(delete_anniversary),
        )
}

fn patch_todo_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::patch(update_todo)
}

fn patch_note_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::patch(update_note)
}

fn patch_anniversary_route() -> axum::routing::MethodRouter<AppState> {
    axum::routing::patch(update_anniversary)
}

// ---------------------------------------------------------------- todos

fn row_todo(row: &rusqlite::Row<'_>) -> Result<SharedTodo, rusqlite::Error> {
    let priority: String = row.get(5)?;
    Ok(SharedTodo {
        id: row.get(0)?,
        creator_id: row.get(1)?,
        title: row.get(2)?,
        detail: row.get(3)?,
        priority: serde_json::from_str(&priority).unwrap_or(Priority::Normal),
        due_at: row.get(4)?,
        completed_at: row.get(6)?,
        completed_by: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}

const TODO_COLUMNS: &str = "id, creator_id, title, detail, due_at, priority, \
     completed_at, completed_by, created_at, updated_at, deleted_at";

async fn list_todos(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<TodoPage>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {TODO_COLUMNS} FROM todos
             WHERE relation_id = ?1 AND deleted_at IS NULL
             ORDER BY created_at DESC LIMIT 500"
        ))
        .map_err(ApiError::from)?;
    let rows = stmt
        .query_map([&relation.id], row_todo)
        .map_err(ApiError::from)?;
    let mut todos = Vec::new();
    for row in rows {
        todos.push(row.map_err(ApiError::from)?);
    }
    Ok(Json(TodoPage { todos }))
}

async fn create_todo(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateTodoRequest>,
) -> Result<(StatusCode, Json<SharedTodo>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    if req.title.trim().is_empty() || req.title.chars().count() > insync_core::limits::TITLE_MAX {
        return Err(ApiError::validation("todo title invalid"));
    }
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let id = uuid::Uuid::now_v7().to_string();
    match conn.execute(
        "INSERT INTO todos (id, relation_id, client_id, creator_id, title, detail, \
             priority, due_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            relation.id,
            req.client_id,
            user.id,
            req.title.trim(),
            req.detail,
            serde_json::to_string(&req.priority).unwrap_or_else(|_| "\"normal\"".into()),
            req.due_at,
            now,
        ],
    ) {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = conn
                .query_row(
                    &format!(
                        "SELECT {TODO_COLUMNS} FROM todos WHERE relation_id = ?1 AND client_id = ?2"
                    ),
                    params![relation.id, req.client_id],
                    row_todo,
                )
                .map_err(ApiError::from)?;
            return Ok((StatusCode::OK, Json(existing)));
        }
        Err(e) => return Err(e.into()),
    }
    let todo = conn
        .query_row(
            &format!("SELECT {TODO_COLUMNS} FROM todos WHERE id = ?1"),
            [&id],
            row_todo,
        )
        .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::TodoCreated,
        &user.id,
        EventData::Todo(Box::new(todo.clone())),
    )?;
    Ok((StatusCode::CREATED, Json(todo)))
}

async fn update_todo(
    State(state): State<AppState>,
    user: AuthUser,
    Path(todo_id): Path<String>,
    Json(req): Json<UpdateTodoRequest>,
) -> Result<Json<SharedTodo>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let current = conn
        .query_row(
            &format!(
                "SELECT {TODO_COLUMNS} FROM todos WHERE id = ?1 AND relation_id = ?2 AND deleted_at IS NULL"
            ),
            params![todo_id, relation.id],
            row_todo,
        )
        .map_err(|_| ApiError::not_found("no such todo"))?;
    let mut todo = current;
    if let Some(title) = &req.title {
        if title.trim().is_empty() || title.chars().count() > insync_core::limits::TITLE_MAX {
            return Err(ApiError::validation("todo title invalid"));
        }
        todo.title = title.trim().to_string();
    }
    if let Some(detail) = &req.detail {
        todo.detail = Some(detail.clone());
    }
    if let Some(priority) = req.priority {
        todo.priority = priority;
    }
    if let Some(due_at) = req.due_at {
        todo.due_at = Some(due_at);
    }
    if let Some(completed) = req.completed {
        if completed && todo.completed_at.is_none() {
            todo.completed_at = Some(now);
            todo.completed_by = Some(user.id.clone());
        } else if !completed {
            todo.completed_at = None;
            todo.completed_by = None;
        }
    }
    todo.updated_at = now;
    conn.execute(
        "UPDATE todos SET title = ?1, detail = ?2, priority = ?3, due_at = ?4,
            completed_at = ?5, completed_by = ?6, updated_at = ?7
         WHERE id = ?8",
        params![
            todo.title,
            todo.detail,
            serde_json::to_string(&todo.priority).unwrap_or_else(|_| "\"normal\"".into()),
            todo.due_at,
            todo.completed_at,
            todo.completed_by,
            todo.updated_at,
            todo.id,
        ],
    )
    .map_err(ApiError::from)?;
    let kind = if req.completed == Some(true) && todo.completed_at == Some(now) {
        EventKind::TodoCompleted
    } else {
        EventKind::TodoUpdated
    };
    events::emit(
        &state,
        &conn,
        &relation.id,
        kind,
        &user.id,
        EventData::Todo(Box::new(todo.clone())),
    )?;
    Ok(Json(todo))
}

async fn delete_todo(
    State(state): State<AppState>,
    user: AuthUser,
    Path(todo_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let deleted = conn
        .execute(
            "UPDATE todos SET deleted_at = ?1, updated_at = ?1
             WHERE id = ?2 AND relation_id = ?3 AND deleted_at IS NULL",
            params![now, todo_id, relation.id],
        )
        .map_err(ApiError::from)?;
    if deleted == 0 {
        return Err(ApiError::not_found("no such todo"));
    }
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::TodoDeleted,
        &user.id,
        EventData::TodoId { todo_id },
    )?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- notes

fn row_note(row: &rusqlite::Row<'_>) -> Result<SharedNote, rusqlite::Error> {
    Ok(SharedNote {
        id: row.get(0)?,
        author_id: row.get(1)?,
        content: row.get(2)?,
        revision: row.get::<_, i64>(3)? as u64,
        updated_by: row.get(4)?,
        updated_at: row.get(5)?,
        created_at: row.get(6)?,
        deleted_at: None,
    })
}

const NOTE_COLUMNS: &str = "id, author_id, content, revision, updated_by, updated_at, created_at";

async fn list_notes(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<NotePage>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM notes
             WHERE relation_id = ?1 AND deleted_at IS NULL
             ORDER BY updated_at DESC LIMIT 200"
        ))
        .map_err(ApiError::from)?;
    let rows = stmt
        .query_map([&relation.id], row_note)
        .map_err(ApiError::from)?;
    let mut notes = Vec::new();
    for row in rows {
        notes.push(row.map_err(ApiError::from)?);
    }
    Ok(Json(NotePage { notes }))
}

async fn create_note(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<SharedNote>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    insync_core::validate_body(&req.content, insync_core::limits::NOTE_BODY_MAX)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let id = uuid::Uuid::now_v7().to_string();
    match conn.execute(
        "INSERT INTO notes (id, relation_id, client_id, author_id, content, \
             revision, updated_by, updated_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, ?4, ?6, ?6)",
        params![id, relation.id, req.client_id, user.id, req.content, now],
    ) {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = conn
                .query_row(
                    &format!(
                        "SELECT {NOTE_COLUMNS} FROM notes WHERE relation_id = ?1 AND client_id = ?2"
                    ),
                    params![relation.id, req.client_id],
                    row_note,
                )
                .map_err(ApiError::from)?;
            return Ok((StatusCode::OK, Json(existing)));
        }
        Err(e) => return Err(e.into()),
    }
    let note = conn
        .query_row(
            &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
            [&id],
            row_note,
        )
        .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::NoteCreated,
        &user.id,
        EventData::Note(Box::new(note.clone())),
    )?;
    Ok((StatusCode::CREATED, Json(note)))
}

/// Optimistic concurrency: a patch against a stale `base_revision` returns
/// 409 with the current note so the user can decide. Nothing is dropped.
async fn update_note(
    State(state): State<AppState>,
    user: AuthUser,
    Path(note_id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> Result<Response, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    insync_core::validate_body(&req.content, insync_core::limits::NOTE_BODY_MAX)
        .map_err(|e| ApiError::validation(e.to_string()))?;
    let current = conn
        .query_row(
            &format!(
                "SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1 AND relation_id = ?2 AND deleted_at IS NULL"
            ),
            params![note_id, relation.id],
            row_note,
        )
        .map_err(|_| ApiError::not_found("no such note"))?;
    if current.revision != req.base_revision {
        return Ok((
            axum::http::StatusCode::CONFLICT,
            Json(NoteConflict { current }),
        )
            .into_response());
    }
    let now = Db::now_ms();
    let new_revision = current.revision + 1;
    conn.execute(
        "UPDATE notes SET content = ?1, revision = ?2, updated_by = ?3, updated_at = ?4
         WHERE id = ?5 AND revision = ?6",
        params![
            req.content,
            new_revision as i64,
            user.id,
            now,
            note_id,
            current.revision as i64
        ],
    )
    .map_err(ApiError::from)?;
    let note = conn
        .query_row(
            &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
            [&note_id],
            row_note,
        )
        .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::NoteUpdated,
        &user.id,
        EventData::Note(Box::new(note.clone())),
    )?;
    Ok((StatusCode::OK, Json(note)).into_response())
}

async fn delete_note(
    State(state): State<AppState>,
    user: AuthUser,
    Path(note_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let deleted = conn
        .execute(
            "UPDATE notes SET deleted_at = ?1, updated_at = ?1
             WHERE id = ?2 AND relation_id = ?3 AND deleted_at IS NULL",
            params![now, note_id, relation.id],
        )
        .map_err(ApiError::from)?;
    if deleted == 0 {
        return Err(ApiError::not_found("no such note"));
    }
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::NoteDeleted,
        &user.id,
        EventData::NoteId { note_id },
    )?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- anniversaries

fn row_anniversary(row: &rusqlite::Row<'_>) -> Result<SharedAnniversary, rusqlite::Error> {
    Ok(SharedAnniversary {
        id: row.get(0)?,
        creator_id: row.get(1)?,
        name: row.get(2)?,
        date: row.get(3)?,
        repeat_yearly: row.get::<_, i64>(4)? != 0,
        note: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        deleted_at: row.get(8)?,
    })
}

const ANNIVERSARY_COLUMNS: &str = "id, creator_id, name, date, repeat_yearly, note, \
     created_at, updated_at, deleted_at";

async fn list_anniversaries(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<AnniversaryPage>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {ANNIVERSARY_COLUMNS} FROM anniversaries
             WHERE relation_id = ?1 AND deleted_at IS NULL
             ORDER BY date ASC LIMIT 500"
        ))
        .map_err(ApiError::from)?;
    let rows = stmt
        .query_map([&relation.id], row_anniversary)
        .map_err(ApiError::from)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(ApiError::from)?);
    }
    Ok(Json(AnniversaryPage {
        anniversaries: items,
    }))
}

async fn create_anniversary(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateAnniversaryRequest>,
) -> Result<(StatusCode, Json<SharedAnniversary>), ApiError> {
    if req.client_id.is_empty() || req.client_id.len() > 64 {
        return Err(ApiError::validation("client_id required"));
    }
    let candidate = SharedAnniversary {
        id: String::new(),
        creator_id: user.id.clone(),
        name: req.name.clone(),
        date: req.date.clone(),
        repeat_yearly: req.repeat_yearly,
        note: req.note.clone(),
        created_at: 0,
        updated_at: 0,
        deleted_at: None,
    };
    candidate
        .validate()
        .map_err(|e| ApiError::validation(e.to_string()))?;
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let id = uuid::Uuid::now_v7().to_string();
    match conn.execute(
        "INSERT INTO anniversaries (id, relation_id, client_id, creator_id, name, date, \
             repeat_yearly, note, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![
            id,
            relation.id,
            req.client_id,
            user.id,
            req.name.trim(),
            req.date,
            req.repeat_yearly as i64,
            req.note,
            now,
        ],
    ) {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => {
            let existing = conn
                .query_row(
                    &format!(
                        "SELECT {ANNIVERSARY_COLUMNS} FROM anniversaries
                         WHERE relation_id = ?1 AND client_id = ?2"
                    ),
                    params![relation.id, req.client_id],
                    row_anniversary,
                )
                .map_err(ApiError::from)?;
            return Ok((StatusCode::OK, Json(existing)));
        }
        Err(e) => return Err(e.into()),
    }
    let anniversary = conn
        .query_row(
            &format!("SELECT {ANNIVERSARY_COLUMNS} FROM anniversaries WHERE id = ?1"),
            [&id],
            row_anniversary,
        )
        .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::AnniversaryCreated,
        &user.id,
        EventData::Anniversary(Box::new(anniversary.clone())),
    )?;
    Ok((StatusCode::CREATED, Json(anniversary)))
}

async fn update_anniversary(
    State(state): State<AppState>,
    user: AuthUser,
    Path(anniversary_id): Path<String>,
    Json(req): Json<UpdateAnniversaryRequest>,
) -> Result<Json<SharedAnniversary>, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let mut item = conn
        .query_row(
            &format!(
                "SELECT {ANNIVERSARY_COLUMNS} FROM anniversaries
                 WHERE id = ?1 AND relation_id = ?2 AND deleted_at IS NULL"
            ),
            params![anniversary_id, relation.id],
            row_anniversary,
        )
        .map_err(|_| ApiError::not_found("no such anniversary"))?;
    if let Some(name) = &req.name {
        item.name = name.trim().to_string();
    }
    if let Some(date) = &req.date {
        item.date = date.clone();
    }
    if let Some(repeat) = req.repeat_yearly {
        item.repeat_yearly = repeat;
    }
    if let Some(note) = &req.note {
        item.note = Some(note.clone());
    }
    item.validate()
        .map_err(|e| ApiError::validation(e.to_string()))?;
    item.updated_at = Db::now_ms();
    conn.execute(
        "UPDATE anniversaries SET name = ?1, date = ?2, repeat_yearly = ?3, note = ?4,
            updated_at = ?5 WHERE id = ?6",
        params![
            item.name,
            item.date,
            item.repeat_yearly as i64,
            item.note,
            item.updated_at,
            item.id,
        ],
    )
    .map_err(ApiError::from)?;
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::AnniversaryUpdated,
        &user.id,
        EventData::Anniversary(Box::new(item.clone())),
    )?;
    Ok(Json(item))
}

async fn delete_anniversary(
    State(state): State<AppState>,
    user: AuthUser,
    Path(anniversary_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let conn = state.db.lock();
    let (relation, _) = require_active_relation(&conn, &user.id)?;
    let now = Db::now_ms();
    let deleted = conn
        .execute(
            "UPDATE anniversaries SET deleted_at = ?1, updated_at = ?1
             WHERE id = ?2 AND relation_id = ?3 AND deleted_at IS NULL",
            params![now, anniversary_id, relation.id],
        )
        .map_err(ApiError::from)?;
    if deleted == 0 {
        return Err(ApiError::not_found("no such anniversary"));
    }
    events::emit(
        &state,
        &conn,
        &relation.id,
        EventKind::AnniversaryDeleted,
        &user.id,
        EventData::AnniversaryId { anniversary_id },
    )?;
    Ok(StatusCode::NO_CONTENT)
}
