use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use lgtm_session::FileReviewStatus;
use crate::AppState;
use crate::routes::threads::persist_session;

#[derive(Deserialize)]
pub struct FileQuery {
    pub path: Option<String>,
}

#[derive(Deserialize)]
pub struct PatchFile {
    pub status: FileReviewStatus,
}

pub async fn patch_file(
    State(state): State<Arc<AppState>>,
    Query(query): Query<FileQuery>,
    Json(body): Json<PatchFile>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let Some(path) = query.path else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "missing 'path' query parameter" })),
        ));
    };

    let mut session = state.session.write().await;
    session.files.insert(path.clone(), body.status);
    session.updated_at = chrono::Utc::now();
    persist_session(&state, &session)?;
    Ok(Json(serde_json::json!({ "path": path, "status": body.status })))
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::create_test_app;

    #[tokio::test]
    async fn test_mark_file_reviewed() {
        let server = create_test_app().await;
        let resp = server
            .patch("/api/files")
            .add_query_param("path", "src/main.rs")
            .json(&serde_json::json!({ "status": "reviewed" }))
            .await;
        resp.assert_status_ok();

        let resp = server.get("/api/session").await;
        let session: lgtm_session::Session = resp.json();
        assert_eq!(
            session.files.get("src/main.rs"),
            Some(&lgtm_session::FileReviewStatus::Reviewed)
        );
    }

    #[tokio::test]
    async fn test_mark_file_pending() {
        let server = create_test_app().await;
        server
            .patch("/api/files")
            .add_query_param("path", "src/main.rs")
            .json(&serde_json::json!({ "status": "reviewed" }))
            .await;
        let resp = server
            .patch("/api/files")
            .add_query_param("path", "src/main.rs")
            .json(&serde_json::json!({ "status": "pending" }))
            .await;
        resp.assert_status_ok();
    }

    #[tokio::test]
    async fn test_missing_path_returns_400() {
        let server = create_test_app().await;
        let resp = server
            .patch("/api/files")
            .json(&serde_json::json!({ "status": "reviewed" }))
            .await;
        resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
}
