use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use lgtm_session::FileReviewStatus;
use crate::AppState;
use crate::routes::sessions::parse_id;

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
    Path(session_id): Path<String>,
    Query(query): Query<FileQuery>,
    Json(body): Json<PatchFile>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;

    let Some(path) = query.path else {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "missing 'path' query parameter" })),
        ));
    };

    let file_path = path.clone();
    let file_status = body.status;
    state.store.update(id, move |s| {
        s.files.insert(file_path, file_status);
    }).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    Ok(Json(serde_json::json!({ "path": path, "status": body.status })))
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
    async fn test_mark_file_reviewed() {
        let (server, session) = test_app_with_session();
        let resp = server
            .patch(&format!("/api/sessions/{}/files", session.id))
            .add_query_param("path", "src/main.rs")
            .json(&serde_json::json!({ "status": "reviewed" }))
            .await;
        resp.assert_status_ok();

        let resp = server.get(&format!("/api/sessions/{}", session.id)).await;
        let s: lgtm_session::Session = resp.json();
        assert_eq!(
            s.files.get("src/main.rs"),
            Some(&lgtm_session::FileReviewStatus::Reviewed)
        );
    }

    #[tokio::test]
    async fn test_mark_file_pending() {
        let (server, session) = test_app_with_session();
        server
            .patch(&format!("/api/sessions/{}/files", session.id))
            .add_query_param("path", "src/main.rs")
            .json(&serde_json::json!({ "status": "reviewed" }))
            .await;
        let resp = server
            .patch(&format!("/api/sessions/{}/files", session.id))
            .add_query_param("path", "src/main.rs")
            .json(&serde_json::json!({ "status": "pending" }))
            .await;
        resp.assert_status_ok();
    }

    #[tokio::test]
    async fn test_missing_path_returns_400() {
        let (server, session) = test_app_with_session();
        let resp = server
            .patch(&format!("/api/sessions/{}/files", session.id))
            .json(&serde_json::json!({ "status": "reviewed" }))
            .await;
        resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
}
