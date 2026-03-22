use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;

use lgtm_git::DiffProvider;
use lgtm_git::cli_provider::CliDiffProvider;
use lgtm_session::{Session, SessionStatus};
use crate::AppState;
use crate::ws::WsMessage;

#[derive(Deserialize)]
pub struct CreateSession {
    pub repo_path: PathBuf,
    pub base: String,
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSession>,
) -> Result<(StatusCode, Json<Session>), (StatusCode, Json<serde_json::Value>)> {
    let provider = CliDiffProvider::new(&body.repo_path);

    let head = provider.head_ref().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let merge_base = provider.merge_base(&head, &body.base).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let session = state.store.create(&body.base, &head, &merge_base, body.repo_path.clone()).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    // Register if not already registered
    let needs_watcher = {
        let providers = state.diff_providers.read().unwrap();
        !providers.contains_key(&session.id)
    };
    if needs_watcher {
        state.register_session(session.id, Box::new(provider));
        // Start file watcher for this repo so diff updates are broadcast
        let _ = crate::watcher::start_watchers(
            state.clone(),
            session.id,
            body.repo_path.clone(),
        );
    }

    Ok((StatusCode::CREATED, Json(session)))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub repo_path: Option<String>,
    pub head: Option<String>,
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ListQuery>,
) -> Json<Vec<Session>> {
    let sessions = state.store.list();
    let filtered: Vec<Session> = sessions
        .into_iter()
        .filter(|s| {
            if let Some(ref rp) = query.repo_path {
                if s.repo_path != std::path::Path::new(rp) {
                    return false;
                }
            }
            if let Some(ref h) = query.head {
                if s.head != *h {
                    return false;
                }
            }
            true
        })
        .collect();
    Json(filtered)
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<Json<Session>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;
    let session = state.store.get(id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    Ok(Json(session))
}

#[derive(Deserialize)]
pub struct PatchSession {
    pub status: SessionStatus,
}

pub async fn patch_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<PatchSession>,
) -> Result<Json<Session>, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;
    let session = state.store.update(id, |s| {
        s.status = body.status;
    }).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    state.broadcast(id, WsMessage::SessionUpdated(session.clone()));
    Ok(Json(session))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    let id = parse_id(&session_id)?;
    state.unregister_session(id);
    state.store.remove(id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn parse_id(s: &str) -> Result<ulid::Ulid, (StatusCode, Json<serde_json::Value>)> {
    s.parse::<ulid::Ulid>().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "invalid session id" })),
        )
    })
}

#[cfg(test)]
mod tests {
    use crate::test_helpers::test_state;
    use lgtm_session::SessionStatus;

    fn create_test_session(state: &std::sync::Arc<crate::AppState>) -> lgtm_session::Session {
        let session = state.store.create("main", "feature/test", "abc1234", std::path::PathBuf::from("/tmp/repo")).unwrap();
        state.register_session(session.id, Box::new(crate::test_helpers::MockDiffProvider));
        session
    }

    fn test_app() -> axum_test::TestServer {
        let state = test_state();
        let app = crate::create_router(state);
        axum_test::TestServer::new(app).unwrap()
    }

    fn test_app_with_session() -> (axum_test::TestServer, lgtm_session::Session) {
        let state = test_state();
        let session = create_test_session(&state);
        let app = crate::create_router(state);
        let server = axum_test::TestServer::new(app).unwrap();
        (server, session)
    }

    #[tokio::test]
    async fn test_list_sessions_empty() {
        let server = test_app();
        let resp = server.get("/api/sessions").await;
        resp.assert_status_ok();
        let sessions: Vec<lgtm_session::Session> = resp.json();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn test_get_session() {
        let (server, session) = test_app_with_session();
        let resp = server.get(&format!("/api/sessions/{}", session.id)).await;
        resp.assert_status_ok();
        let fetched: lgtm_session::Session = resp.json();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.status, SessionStatus::InProgress);
    }

    #[tokio::test]
    async fn test_patch_session_approve() {
        let (server, session) = test_app_with_session();
        let resp = server
            .patch(&format!("/api/sessions/{}", session.id))
            .json(&serde_json::json!({ "status": "approved" }))
            .await;
        resp.assert_status_ok();
        let updated: lgtm_session::Session = resp.json();
        assert_eq!(updated.status, SessionStatus::Approved);
    }

    #[tokio::test]
    async fn test_patch_session_invalid_status() {
        let (server, session) = test_app_with_session();
        let resp = server
            .patch(&format!("/api/sessions/{}", session.id))
            .json(&serde_json::json!({ "status": "invalid" }))
            .await;
        resp.assert_status(axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn test_delete_session() {
        let (server, session) = test_app_with_session();
        let resp = server.delete(&format!("/api/sessions/{}", session.id)).await;
        resp.assert_status(axum::http::StatusCode::NO_CONTENT);
        let resp = server.get(&format!("/api/sessions/{}", session.id)).await;
        resp.assert_status(axum::http::StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_nonexistent_session() {
        let server = test_app();
        let resp = server.get("/api/sessions/00000000000000000000000000").await;
        resp.assert_status(axum::http::StatusCode::NOT_FOUND);
    }
}
