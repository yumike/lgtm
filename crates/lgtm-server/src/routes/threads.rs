use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use lgtm_session::{Author, Comment, DiffSide, Origin, Severity, Thread, ThreadStatus};
use crate::AppState;
use crate::routes::sessions::parse_id;

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
    Path(session_id): Path<String>,
    Json(body): Json<CreateThread>,
) -> Result<Json<Thread>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;
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
            diff_snapshot: None,
        }],
    };

    let thread_clone = thread.clone();
    state.store.update(id, |s| {
        s.threads.push(thread_clone);
    }).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(thread))
}

#[derive(Deserialize)]
pub struct AddComment {
    pub body: String,
}

pub async fn add_comment(
    State(state): State<Arc<AppState>>,
    Path((session_id, thread_id)): Path<(String, String)>,
    Json(body): Json<AddComment>,
) -> Result<Json<Comment>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;
    let now = chrono::Utc::now();

    let comment = Comment {
        id: ulid::Ulid::new().to_string(),
        author: Author::Developer,
        body: body.body,
        timestamp: now,
        diff_snapshot: None,
    };

    let comment_clone = comment.clone();
    let tid = thread_id.clone();
    let session = state.store.update(id, move |s| {
        if let Some(thread) = s.threads.iter_mut().find(|t| t.id == tid) {
            thread.comments.push(comment_clone);
        }
    }).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    // Check if thread was actually found
    let thread = session.threads.iter().find(|t| t.id == thread_id);
    if thread.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "thread not found" })),
        ));
    }

    Ok(Json(comment))
}

#[derive(Deserialize)]
pub struct PatchThread {
    pub status: ThreadStatus,
}

pub async fn patch_thread(
    State(state): State<Arc<AppState>>,
    Path((session_id, thread_id)): Path<(String, String)>,
    Json(body): Json<PatchThread>,
) -> Result<Json<Thread>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;

    // First get session to validate
    let session = state.store.get(id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

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

    let tid = thread_id.clone();
    let status = body.status;
    let updated = state.store.update(id, move |s| {
        if let Some(thread) = s.threads.iter_mut().find(|t| t.id == tid) {
            thread.status = status;
        }
    }).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let thread = updated.threads.iter().find(|t| t.id == thread_id).unwrap().clone();
    Ok(Json(thread))
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::{test_state, MockDiffProvider};

    fn test_app_with_session() -> (axum_test::TestServer, lgtm_session::Session) {
        let state = test_state();
        let session = state.store.create("main", "feature/test", "abc1234", std::path::PathBuf::from("/tmp/repo")).unwrap();
        state.register_session(session.id, Box::new(MockDiffProvider));
        let app = crate::create_router(state);
        let server = axum_test::TestServer::new(app).unwrap();
        (server, session)
    }

    #[tokio::test]
    async fn test_create_thread() {
        let (server, session) = test_app_with_session();
        let resp = server
            .post(&format!("/api/sessions/{}/threads", session.id))
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
        let (server, session) = test_app_with_session();
        let resp = server
            .post(&format!("/api/sessions/{}/threads", session.id))
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
            .post(&format!("/api/sessions/{}/threads/{}/comments", session.id, thread.id))
            .json(&serde_json::json!({
                "body": "Reply comment"
            }))
            .await;
        resp.assert_status_ok();
    }

    #[tokio::test]
    async fn test_patch_thread_resolve() {
        let (server, session) = test_app_with_session();
        let resp = server
            .post(&format!("/api/sessions/{}/threads", session.id))
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
            .patch(&format!("/api/sessions/{}/threads/{}", session.id, thread.id))
            .json(&serde_json::json!({ "status": "resolved" }))
            .await;
        resp.assert_status_ok();
        let updated: lgtm_session::Thread = resp.json();
        assert_eq!(updated.status, lgtm_session::ThreadStatus::Resolved);
    }

    #[tokio::test]
    async fn test_patch_nonexistent_thread_returns_404() {
        let (server, session) = test_app_with_session();
        let resp = server
            .patch(&format!("/api/sessions/{}/threads/nonexistent", session.id))
            .json(&serde_json::json!({ "status": "resolved" }))
            .await;
        resp.assert_status(axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_create_agent_thread() {
        let (server, session) = test_app_with_session();
        let resp = server
            .post(&format!("/api/sessions/{}/threads", session.id))
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
        let (server, session) = test_app_with_session();
        let resp = server
            .post(&format!("/api/sessions/{}/threads", session.id))
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
            .patch(&format!("/api/sessions/{}/threads/{}", session.id, thread.id))
            .json(&serde_json::json!({ "status": "dismissed" }))
            .await;
        resp.assert_status_ok();
        let updated: lgtm_session::Thread = resp.json();
        assert_eq!(updated.status, lgtm_session::ThreadStatus::Dismissed);
    }

    #[tokio::test]
    async fn test_dismiss_developer_thread_rejected() {
        let (server, session) = test_app_with_session();
        let resp = server
            .post(&format!("/api/sessions/{}/threads", session.id))
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
            .patch(&format!("/api/sessions/{}/threads/{}", session.id, thread.id))
            .json(&serde_json::json!({ "status": "dismissed" }))
            .await;
        resp.assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }
}
