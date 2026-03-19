use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;

use lgtm_session::{Session, SessionStatus};
use crate::AppState;

pub async fn get_session(
    State(state): State<Arc<AppState>>,
) -> Json<Session> {
    let session = state.session.read().await;
    Json(session.clone())
}

#[derive(Deserialize)]
pub struct PatchSession {
    pub status: SessionStatus,
}

pub async fn patch_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PatchSession>,
) -> Result<Json<Session>, (StatusCode, Json<serde_json::Value>)> {
    let mut session = state.session.write().await;
    session.status = body.status;
    session.updated_at = chrono::Utc::now();
    let lock_path = state.session_path.with_file_name(".lock");
    let _lock = lgtm_session::acquire_lock(&lock_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    if let Err(e) = lgtm_session::write_session_atomic(&state.session_path, &session) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ));
    }
    Ok(Json(session.clone()))
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::create_test_app;

    #[tokio::test]
    async fn test_get_session() {
        let server = create_test_app().await;
        let resp = server.get("/api/session").await;
        resp.assert_status_ok();
        let session: lgtm_session::Session = resp.json();
        assert_eq!(session.status, lgtm_session::SessionStatus::InProgress);
    }

    #[tokio::test]
    async fn test_patch_session_approve() {
        let server = create_test_app().await;
        let resp = server
            .patch("/api/session")
            .json(&serde_json::json!({ "status": "approved" }))
            .await;
        resp.assert_status_ok();
        let resp = server.get("/api/session").await;
        let session: lgtm_session::Session = resp.json();
        assert_eq!(session.status, lgtm_session::SessionStatus::Approved);
    }

    #[tokio::test]
    async fn test_patch_session_invalid_status() {
        let server = create_test_app().await;
        let resp = server
            .patch("/api/session")
            .json(&serde_json::json!({ "status": "invalid" }))
            .await;
        resp.assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }
}
