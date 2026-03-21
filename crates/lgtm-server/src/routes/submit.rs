use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;

use crate::AppState;
use crate::routes::sessions::parse_id;
use crate::ws::{WsMessage, SubmitStatusData};

#[derive(Serialize)]
pub struct SubmitResponse {
    pub pending: bool,
}

pub async fn post_submit(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<(StatusCode, Json<SubmitResponse>), (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;

    // Verify session exists
    state.store.get(id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let mut pending = state.submit_pending.write().unwrap();
    if *pending.get(&id).unwrap_or(&false) {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Submit already pending" })),
        ));
    }
    pending.insert(id, true);
    drop(pending);

    state.broadcast(id, WsMessage::SubmitStatus(SubmitStatusData { pending: true }));
    Ok((StatusCode::CREATED, Json(SubmitResponse { pending: true })))
}

pub async fn get_submit(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<SubmitResponse>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;

    // Verify session exists
    state.store.get(id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let pending = state.submit_pending.read().unwrap();
    let is_pending = *pending.get(&id).unwrap_or(&false);
    Ok(Json(SubmitResponse { pending: is_pending }))
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
    async fn test_post_submit_creates_marker() {
        let (server, session) = test_app_with_session();
        let resp = server.post(&format!("/api/sessions/{}/submit", session.id)).await;
        resp.assert_status(axum::http::StatusCode::CREATED);
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], true);
    }

    #[tokio::test]
    async fn test_post_submit_conflict_when_pending() {
        let (server, session) = test_app_with_session();
        server.post(&format!("/api/sessions/{}/submit", session.id)).await;
        let resp = server.post(&format!("/api/sessions/{}/submit", session.id)).await;
        resp.assert_status(axum::http::StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_get_submit_false_initially() {
        let (server, session) = test_app_with_session();
        let resp = server.get(&format!("/api/sessions/{}/submit", session.id)).await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], false);
    }

    #[tokio::test]
    async fn test_get_submit_true_after_post() {
        let (server, session) = test_app_with_session();
        server.post(&format!("/api/sessions/{}/submit", session.id)).await;
        let resp = server.get(&format!("/api/sessions/{}/submit", session.id)).await;
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], true);
    }
}
