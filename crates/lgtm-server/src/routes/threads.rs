use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use lgtm_session::{Author, Comment, DiffSide, Origin, Severity, Thread, ThreadStatus};
use crate::AppState;

#[derive(Deserialize)]
pub struct CreateThread {
    pub file: String,
    pub line_start: u32,
    pub line_end: u32,
    pub diff_side: DiffSide,
    pub anchor_context: String,
    pub body: String,
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub severity: Option<Severity>,
}

pub async fn create_thread(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateThread>,
) -> Result<Json<Thread>, (StatusCode, Json<serde_json::Value>)> {
    let mut session = state.session.write().await;
    let now = chrono::Utc::now();

    let author = match body.origin {
        Origin::Agent => Author::Agent,
        Origin::Developer => Author::Developer,
    };

    let thread = Thread {
        id: ulid::Ulid::new().to_string(),
        origin: body.origin,
        severity: body.severity,
        status: ThreadStatus::Open,
        file: body.file,
        line_start: body.line_start,
        line_end: body.line_end,
        diff_side: body.diff_side,
        anchor_context: body.anchor_context,
        comments: vec![Comment {
            id: ulid::Ulid::new().to_string(),
            author,
            body: body.body,
            timestamp: now,
        }],
    };

    session.threads.push(thread.clone());
    session.updated_at = now;
    persist_session(&state, &session)?;
    Ok(Json(thread))
}

#[derive(Deserialize)]
pub struct AddComment {
    pub body: String,
}

pub async fn add_comment(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(body): Json<AddComment>,
) -> Result<Json<Comment>, (StatusCode, Json<serde_json::Value>)> {
    let mut session = state.session.write().await;
    let now = chrono::Utc::now();

    let thread = session.threads.iter_mut().find(|t| t.id == thread_id);
    let Some(thread) = thread else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "thread not found" })),
        ));
    };

    let comment = Comment {
        id: ulid::Ulid::new().to_string(),
        author: Author::Developer,
        body: body.body,
        timestamp: now,
    };

    thread.comments.push(comment.clone());
    session.updated_at = now;
    persist_session(&state, &session)?;
    Ok(Json(comment))
}

#[derive(Deserialize)]
pub struct PatchThread {
    pub status: ThreadStatus,
}

pub async fn patch_thread(
    State(state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(body): Json<PatchThread>,
) -> Result<Json<Thread>, (StatusCode, Json<serde_json::Value>)> {
    let mut session = state.session.write().await;
    let now = chrono::Utc::now();

    let thread_idx = session.threads.iter().position(|t| t.id == thread_id);
    let Some(idx) = thread_idx else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "thread not found" })),
        ));
    };

    // Dismissed is only valid for agent-origin threads
    if body.status == ThreadStatus::Dismissed && session.threads[idx].origin != Origin::Agent {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "error": "dismissed status is only valid for agent-initiated threads" })),
        ));
    }

    session.threads[idx].status = body.status;
    session.updated_at = now;
    let thread = session.threads[idx].clone();
    persist_session(&state, &session)?;
    Ok(Json(thread))
}

pub fn persist_session(
    state: &AppState,
    session: &lgtm_session::Session,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    lgtm_session::write_session(&state.session_path, session).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::create_test_app;

    #[tokio::test]
    async fn test_create_thread() {
        let server = create_test_app().await;
        let resp = server
            .post("/api/threads")
            .json(&serde_json::json!({
                "file": "src/main.rs",
                "line_start": 10,
                "line_end": 10,
                "diff_side": "right",
                "anchor_context": "fn main() {",
                "body": "This needs error handling"
            }))
            .await;
        resp.assert_status_ok();
        let thread: lgtm_session::Thread = resp.json();
        assert_eq!(thread.file, "src/main.rs");
        assert_eq!(thread.comments.len(), 1);
        assert_eq!(thread.comments[0].body, "This needs error handling");
    }

    #[tokio::test]
    async fn test_add_comment_to_thread() {
        let server = create_test_app().await;
        let resp = server
            .post("/api/threads")
            .json(&serde_json::json!({
                "file": "src/main.rs",
                "line_start": 10,
                "line_end": 10,
                "diff_side": "right",
                "anchor_context": "fn main() {",
                "body": "Initial comment"
            }))
            .await;
        let thread: lgtm_session::Thread = resp.json();

        let resp = server
            .post(&format!("/api/threads/{}/comments", thread.id))
            .json(&serde_json::json!({
                "body": "Reply comment"
            }))
            .await;
        resp.assert_status_ok();
    }

    #[tokio::test]
    async fn test_patch_thread_resolve() {
        let server = create_test_app().await;
        let resp = server
            .post("/api/threads")
            .json(&serde_json::json!({
                "file": "src/main.rs",
                "line_start": 10,
                "line_end": 10,
                "diff_side": "right",
                "anchor_context": "fn main() {",
                "body": "Fix this"
            }))
            .await;
        let thread: lgtm_session::Thread = resp.json();

        let resp = server
            .patch(&format!("/api/threads/{}", thread.id))
            .json(&serde_json::json!({ "status": "resolved" }))
            .await;
        resp.assert_status_ok();
        let updated: lgtm_session::Thread = resp.json();
        assert_eq!(updated.status, lgtm_session::ThreadStatus::Resolved);
    }

    #[tokio::test]
    async fn test_patch_nonexistent_thread_returns_404() {
        let server = create_test_app().await;
        let resp = server
            .patch("/api/threads/nonexistent")
            .json(&serde_json::json!({ "status": "resolved" }))
            .await;
        resp.assert_status(axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_agent_thread() {
        let server = create_test_app().await;
        let resp = server
            .post("/api/threads")
            .json(&serde_json::json!({
                "file": "src/main.rs",
                "line_start": 5,
                "line_end": 5,
                "diff_side": "right",
                "anchor_context": "API_KEY = \"secret\"",
                "body": "Hardcoded API key detected",
                "origin": "agent",
                "severity": "warning"
            }))
            .await;
        resp.assert_status_ok();
        let thread: lgtm_session::Thread = resp.json();
        assert_eq!(thread.origin, lgtm_session::Origin::Agent);
        assert_eq!(thread.severity, Some(lgtm_session::Severity::Warning));
        assert_eq!(thread.comments[0].author, lgtm_session::Author::Agent);
    }

    #[tokio::test]
    async fn test_dismiss_agent_thread() {
        let server = create_test_app().await;
        let resp = server
            .post("/api/threads")
            .json(&serde_json::json!({
                "file": "src/main.rs",
                "line_start": 5,
                "line_end": 5,
                "diff_side": "right",
                "anchor_context": "test",
                "body": "Agent observation",
                "origin": "agent",
                "severity": "info"
            }))
            .await;
        let thread: lgtm_session::Thread = resp.json();

        let resp = server
            .patch(&format!("/api/threads/{}", thread.id))
            .json(&serde_json::json!({ "status": "dismissed" }))
            .await;
        resp.assert_status_ok();
        let updated: lgtm_session::Thread = resp.json();
        assert_eq!(updated.status, lgtm_session::ThreadStatus::Dismissed);
    }

    #[tokio::test]
    async fn test_dismiss_developer_thread_rejected() {
        let server = create_test_app().await;
        let resp = server
            .post("/api/threads")
            .json(&serde_json::json!({
                "file": "src/main.rs",
                "line_start": 5,
                "line_end": 5,
                "diff_side": "right",
                "anchor_context": "test",
                "body": "Developer comment"
            }))
            .await;
        let thread: lgtm_session::Thread = resp.json();

        let resp = server
            .patch(&format!("/api/threads/{}", thread.id))
            .json(&serde_json::json!({ "status": "dismissed" }))
            .await;
        resp.assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }
}
