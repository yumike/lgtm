use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;

use crate::AppState;
use crate::ws::{WsMessage, SubmitStatusData};

#[derive(Serialize)]
pub struct SubmitResponse {
    pub pending: bool,
}

pub async fn post_submit(
    State(state): State<Arc<AppState>>,
) -> Result<(StatusCode, Json<SubmitResponse>), (StatusCode, Json<serde_json::Value>)> {
    let submit_path = state.session_path.parent().unwrap().join(".submit");
    // Use create_new(true) for O_CREAT | O_EXCL semantics — atomic, no TOCTOU race
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&submit_path) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Submit already pending" })),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            ));
        }
        Ok(_) => {}
    }
    let _ = state.broadcast_tx.send(WsMessage::SubmitStatus(SubmitStatusData { pending: true }));
    Ok((StatusCode::CREATED, Json(SubmitResponse { pending: true })))
}

pub async fn get_submit(
    State(state): State<Arc<AppState>>,
) -> Json<SubmitResponse> {
    let submit_path = state.session_path.parent().unwrap().join(".submit");
    Json(SubmitResponse { pending: submit_path.exists() })
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::create_test_app;

    #[tokio::test]
    async fn test_post_submit_creates_marker() {
        let server = create_test_app().await;
        let resp = server.post("/api/submit").await;
        resp.assert_status(axum::http::StatusCode::CREATED);
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], true);
    }

    #[tokio::test]
    async fn test_post_submit_conflict_when_pending() {
        let server = create_test_app().await;
        server.post("/api/submit").await;
        let resp = server.post("/api/submit").await;
        resp.assert_status(axum::http::StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_get_submit_false_initially() {
        let server = create_test_app().await;
        let resp = server.get("/api/submit").await;
        resp.assert_status_ok();
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], false);
    }

    #[tokio::test]
    async fn test_get_submit_true_after_post() {
        let server = create_test_app().await;
        server.post("/api/submit").await;
        let resp = server.get("/api/submit").await;
        let body: serde_json::Value = resp.json();
        assert_eq!(body["pending"], true);
    }
}
